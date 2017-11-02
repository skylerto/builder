// Copyright (c) 2016-2017 Chef Software Inc. and/or applicable contributors
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

#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]

extern crate airlock;
#[macro_use]
extern crate clap;
extern crate env_logger;
#[macro_use]
extern crate log;
extern crate tempdir;

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::result;

use airlock::command;
use airlock::{Error, Result};
use clap::{App, ArgMatches};
use tempdir::TempDir;

const FS_ROOT_ENVVAR: &'static str = "AIRLOCK_FS_ROOT";
const VERSION: &'static str = include_str!(concat!(env!("OUT_DIR"), "/VERSION"));

fn main() {
    env_logger::init().unwrap();
    if let Err(e) = _main() {
        eprintln!("FATAL: {}", e);
        process::exit(1);
    }
}

fn _main() -> Result<()> {
    let app_matches = cli().get_matches();
    debug!("clap cli matches: {:?}", &app_matches);
    match app_matches.subcommand() {
        ("nsrun", Some(m)) => sub_nsrun(m),
        ("run", Some(m)) => sub_run(m),
        _ => unreachable!(),
    }
}

fn sub_nsrun(m: &ArgMatches) -> Result<()> {
    let rootfs = Path::new(m.value_of("FS_ROOT").unwrap());
    let mut args: Vec<&OsStr> = m.values_of_os("CMD").unwrap().collect();
    // cmd arg is required and multiple so must contain a first element
    let cmd = args.remove(0);

    command::nsrun::run(rootfs, cmd, args)
}

fn sub_run(m: &ArgMatches) -> Result<()> {
    let mut args: Vec<&OsStr> = m.values_of_os("CMD").unwrap().collect();
    // cmd arg is required and multiple so must contain a first element
    let cmd = args.remove(0);

    match env::var(FS_ROOT_ENVVAR) {
        Ok(ref val) => {
            let rootfs = Path::new(val);
            if rootfs.exists() {
                return Err(Error::Rootfs(val.to_string()));
            }
            fs::create_dir(&rootfs)?;
            command::run::run(cmd, args, rootfs)
        }
        Err(_) => {
            let tmpdir = TempDir::new("rootfs")?;
            command::run::run(cmd, args, tmpdir.path())
        }
    }
}

fn cli<'a, 'b>() -> App<'a, 'b> {
    let program_name = {
        let arg0 = env::args().next().map(|p| PathBuf::from(p));
        arg0.as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|p| p.to_str())
            .unwrap()
            .to_string()
    };
    clap_app!((program_name) =>
        (about: "Airlock: your gateway to a Studio")
        (version: VERSION)
        (author: "\nAuthors: The Habitat Maintainers <humans@habitat.sh>\n\n")
        (@setting VersionlessSubcommands)
        (@setting ArgRequiredElseHelp)
        (@subcommand nsrun =>
            (@setting Hidden)
            (about: "**Internal** command to run a command inside the created namespace")
            (@setting TrailingVarArg)
            (@arg FS_ROOT: +required +takes_value {dir_exists}
                "Path to the rootfs (ex: /tmp/rootfs)")
            (@arg CMD: +required +takes_value +multiple
                "The command and arguments to execute (ex: ls -l /tmp)")
        )
        (@subcommand run =>
            (about: "Run a command in a namespace")
            (@setting TrailingVarArg)
            (@arg CMD: +required +takes_value +multiple
                "The command and arguments to execute (ex: ls -l /tmp)")
        )
    )
}

fn dir_exists(val: String) -> result::Result<(), String> {
    if Path::new(&val).is_dir() {
        Ok(())
    } else {
        Err(format!("Directory: '{}' cannot be found", &val))
    }
}
