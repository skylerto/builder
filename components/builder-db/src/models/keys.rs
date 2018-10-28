use super::db_id_format;
use chrono::NaiveDateTime;
use diesel;
use diesel::pg::PgConnection;
use diesel::result::QueryResult;
use diesel::sql_types::{BigInt, Binary, Text};
use diesel::RunQueryDsl;
use schema::key::*;

#[derive(Debug, Serialize, Deserialize, QueryableByName)]
#[table_name = "origin_public_encryption_keys"]
pub struct PublicEncryptionKey {
    #[serde(with = "db_id_format")]
    pub id: i64,
    #[serde(with = "db_id_format")]
    pub origin_id: i64,
    #[serde(with = "db_id_format")]
    pub owner_id: i64,
    pub name: String,
    pub revision: String,
    pub full_name: String,
    pub body: Vec<u8>,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
}

#[derive(Debug, Serialize, Deserialize, QueryableByName)]
#[table_name = "origin_private_encryption_keys"]
pub struct PrivateEncryptionKey {
    #[serde(with = "db_id_format")]
    pub id: i64,
    #[serde(with = "db_id_format")]
    pub origin_id: i64,
    #[serde(with = "db_id_format")]
    pub owner_id: i64,
    pub name: String,
    pub revision: String,
    pub full_name: String,
    pub body: Vec<u8>,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
}

#[derive(Insertable)]
#[table_name = "origin_public_encryption_keys"]
pub struct NewPublicEncryptionKey<'a> {
    pub owner_id: i64,
    pub origin_id: i64,
    pub name: &'a str,
    pub revision: &'a str,
    pub body: &'a [u8],
}

#[derive(Insertable)]
#[table_name = "origin_private_encryption_keys"]
pub struct NewPrivateEncryptionKey<'a> {
    pub owner_id: i64,
    pub origin_id: i64,
    pub name: &'a str,
    pub revision: &'a str,
    pub body: &'a [u8],
}

impl PublicEncryptionKey {
    pub fn get(
        origin: &str,
        revision: &str,
        conn: &PgConnection,
    ) -> QueryResult<PublicEncryptionKey> {
        diesel::sql_query("select * from get_origin_public_encryption_key_v1($1, $2)")
            .bind::<Text, _>(origin)
            .bind::<Text, _>(revision)
            .get_result(conn)
    }

    pub fn create(
        req: &NewPublicEncryptionKey,
        conn: &PgConnection,
    ) -> QueryResult<PublicEncryptionKey> {
        let full_name = format!("{}-{}", req.name, req.revision);
        diesel::sql_query(
            "select * from insert_origin_public_encryption_key_v1($1, $2, $3, $4, $5, $6)",
        ).bind::<BigInt, _>(req.origin_id)
        .bind::<BigInt, _>(req.owner_id)
        .bind::<Text, _>(req.name)
        .bind::<Text, _>(req.revision)
        .bind::<Text, _>(full_name)
        .bind::<Binary, _>(req.body)
        .get_result(conn)
    }

    pub fn latest(origin: &str, conn: &PgConnection) -> QueryResult<PublicEncryptionKey> {
        diesel::sql_query("select * from get_origin_public_encryption_key_latest_v1($1)")
            .bind::<Text, _>(origin)
            .get_result(conn)
    }

    pub fn list(origin: &str, conn: &PgConnection) -> QueryResult<Vec<PublicEncryptionKey>> {
        diesel::sql_query("select * from get_origin_public_encryption_keys_for_origin_v1($1)")
            .bind::<Text, _>(origin)
            .get_results(conn)
    }
}

impl PrivateEncryptionKey {
    pub fn get(origin: &str, conn: &PgConnection) -> QueryResult<PrivateEncryptionKey> {
        diesel::sql_query("select * from get_origin_private_encryption_key_v1($1)")
            .bind::<Text, _>(origin)
            .get_result(conn)
    }

    pub fn create(
        req: &NewPrivateEncryptionKey,
        conn: &PgConnection,
    ) -> QueryResult<PrivateEncryptionKey> {
        let full_name = format!("{}-{}", req.name, req.revision);
        diesel::sql_query(
            "select * from insert_origin_private_encryption_key_v1($1, $2, $3, $4, $5, $6)",
        ).bind::<BigInt, _>(req.origin_id)
        .bind::<BigInt, _>(req.owner_id)
        .bind::<Text, _>(req.name)
        .bind::<Text, _>(req.revision)
        .bind::<Text, _>(full_name)
        .bind::<Binary, _>(req.body)
        .get_result(conn)
    }
}
