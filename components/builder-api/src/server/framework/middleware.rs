// Copyright (c) 2016 Chef Software Inc. and/or applicable contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::env;

use actix_web::http;
use actix_web::middleware::{Middleware, Started};
use actix_web::{HttpRequest, HttpResponse, Result};

use base64;
use oauth_client::types::OAuth2User;
use protobuf;

use crate::bldr_core;
use crate::bldr_core::access_token::{BUILDER_ACCOUNT_ID, BUILDER_ACCOUNT_NAME};
use crate::bldr_core::metrics::CounterMetric;
use crate::bldr_core::privilege::FeatureFlags;

use crate::db::models::account::*;
use crate::protocol;
use crate::protocol::originsrv;

use crate::server::error;
use crate::server::services::metrics::Counter;
use crate::server::AppState;

lazy_static! {
    static ref SESSION_DURATION: u32 = 3 * 24 * 60 * 60;
}

pub fn route_message<R, T>(req: &HttpRequest<AppState>, msg: &R) -> error::Result<T>
where
    R: protobuf::Message,
    T: protobuf::Message,
{
    Counter::RouteMessage.increment();
    // Route via Protobuf over HTTP
    req.state()
        .jobsrv
        .rpc::<R, T>(msg)
        .map_err(error::Error::BuilderCore)
}

// Optional Authentication - this middleware does not enforce authentication,
// but will insert a Session if a valid Bearer token is received
pub struct Authentication;

impl Middleware<AppState> for Authentication {
    fn start(&self, req: &HttpRequest<AppState>) -> Result<Started> {
        let hdr = match req.headers().get(http::header::AUTHORIZATION) {
            Some(hdr) => hdr.to_str().unwrap(), // unwrap Ok
            None => return Ok(Started::Done),
        };

        let hdr_components: Vec<&str> = hdr.split_whitespace().collect();
        if (hdr_components.len() != 2) || (hdr_components[0] != "Bearer") {
            return Ok(Started::Response(HttpResponse::Unauthorized().finish()));
        }
        let token = hdr_components[1];

        let session = match authenticate(req, &token) {
            Ok(session) => session,
            Err(_) => return Ok(Started::Response(HttpResponse::Unauthorized().finish())),
        };

        req.extensions_mut().insert::<originsrv::Session>(session);
        Ok(Started::Done)
    }
}

fn authenticate(req: &HttpRequest<AppState>, token: &str) -> error::Result<originsrv::Session> {
    // Test hook - always create a valid session
    if env::var_os("HAB_FUNC_TEST").is_some() {
        debug!(
            "HAB_FUNC_TEST: {:?}; calling session_create_short_circuit",
            env::var_os("HAB_FUNC_TEST")
        );
        return session_create_short_circuit(req, token);
    };

    let mut memcache = req.state().memcache.borrow_mut();
    match memcache.get_session(token) {
        Some(session) => {
            trace!("Session {} Cache Hit!", token);
            return Ok(session);
        }
        None => {
            trace!("Session {} Cache Miss!", token);
            if !bldr_core::access_token::is_access_token(token) {
                // No token in cache and not a PAT - bail
                return Err(error::Error::Authorization);
            }
            // Pull the session out of the current token provided so we can validate
            // it against the db's tokens
            let mut session = bldr_core::access_token::validate_access_token(
                &req.state().config.api.key_path,
                token,
            )
            .map_err(|_| error::Error::Authorization)?;

            if session.get_id() == BUILDER_ACCOUNT_ID {
                trace!("Builder token identified");
                session.set_name(BUILDER_ACCOUNT_NAME.to_owned());
                memcache.set_session(token, &session, None);
                return Ok(session);
            }

            // If we can't find a token in the cache, we need to round-trip to the
            // db to see if we have a valid session token.
            let conn = req.state().db.get_conn().map_err(error::Error::DbError)?;

            match AccountToken::list(session.get_id(), &*conn).map_err(error::Error::DieselError) {
                Ok(access_tokens) => {
                    assert!(access_tokens.len() <= 1); // Can only have max of 1 for now
                    match access_tokens.first() {
                        Some(access_token) => {
                            let new_token = access_token.token.clone();
                            if token.trim_right_matches('=') != new_token.trim_right_matches('=') {
                                // Token is valid but revoked or otherwise expired
                                return Err(error::Error::Authorization);
                            }

                            let account = Account::get_by_id(session.get_id() as i64, &*conn)
                                .map_err(error::Error::DieselError)?;
                            session.set_name(account.name);
                            session.set_email(account.email);

                            memcache.set_session(&new_token, &session, None);
                            return Ok(session);
                        }
                        None => {
                            // We have no tokens in the database for this user
                            return Err(error::Error::Authorization);
                        }
                    }
                }
                Err(_) => {
                    // Failed to fetch tokens from the database for this user
                    return Err(error::Error::Authorization);
                }
            }
        }
    }
}

pub fn session_create_oauth(
    req: &HttpRequest<AppState>,
    oauth_token: &str,
    user: &OAuth2User,
    provider: &str,
) -> error::Result<originsrv::Session> {
    let mut session = originsrv::Session::new();
    let mut session_token = originsrv::SessionToken::new();
    let conn = req.state().db.get_conn().map_err(error::Error::DbError)?;

    let email = match user.email {
        Some(ref email) => {
            session.set_email(email.clone());
            email
        }
        None => "",
    };

    match Account::find_or_create(
        &NewAccount {
            name: &user.username,
            email: email,
        },
        &*conn,
    ) {
        Ok(account) => {
            session_token.set_account_id(account.id as u64);
            session_token.set_extern_id(user.id.to_string());
            session_token.set_token(oauth_token.to_string().into_bytes());

            match provider.parse::<originsrv::OAuthProvider>() {
                Ok(p) => session_token.set_provider(p),
                Err(e) => {
                    warn!(
                        "Error parsing oauth provider: provider={}, err={:?}",
                        provider, e
                    );
                    return Err(error::Error::System);
                }
            }

            let encoded_token = encode_token(&session_token);
            session.set_id(account.id as u64);
            session.set_name(account.name);
            session.set_token(encoded_token.clone());
            session.set_flags(FeatureFlags::empty().bits());
            session.set_oauth_token(oauth_token.to_owned());

            debug!("issuing session, {:?}", session);
            req.state().memcache.borrow_mut().set_session(
                &session.get_token(),
                &session,
                Some(*SESSION_DURATION),
            );
            Ok(session)
        }
        Err(e) => {
            error!("Failed to create session {}", e);
            Err(e.into())
        }
    }
}

pub fn session_create_short_circuit(
    req: &HttpRequest<AppState>,
    token: &str,
) -> error::Result<originsrv::Session> {
    let (user, provider) = match token.as_ref() {
        "bobo" => (
            OAuth2User {
                id: "0".to_string(),
                email: Some("bobo@example.com".to_string()),
                username: "bobo".to_string(),
            },
            "GitHub",
        ),
        "mystique" => (
            OAuth2User {
                id: "1".to_string(),
                email: Some("mystique@example.com".to_string()),
                username: "mystique".to_string(),
            },
            "GitHub",
        ),
        "hank" => (
            OAuth2User {
                id: "2".to_string(),
                email: Some("hank@example.com".to_string()),
                username: "hank".to_string(),
            },
            "GitHub",
        ),
        user => {
            error!("Unexpected short circuit token {:?}", user);
            return Err(error::Error::System);
        }
    };

    session_create_oauth(req, token, &user, provider)
}

fn encode_token(token: &originsrv::SessionToken) -> String {
    let bytes = protocol::message::encode(token).unwrap(); //Unwrap is safe
    base64::encode(&bytes)
}
