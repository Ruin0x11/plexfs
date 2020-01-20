extern crate anyhow;
extern crate clap;
extern crate env_logger;
extern crate fuse;
extern crate libc;
extern crate quick_xml;
extern crate reqwest;
extern crate serde;
extern crate time;
#[macro_use] extern crate log;

mod api;
mod fs;

use std::env;
use std::ffi::OsStr;
use clap::{App, Arg, crate_version};

fn app<'a, 'b>() -> App<'a, 'b> {
    App::new(format!("plexfs {}", crate_version!()))
        .about("Mount a Plex server as a local filesystem.")
        .arg(Arg::with_name("version").short("v").long("version").help(
            "Prints version info",
        ))
        .arg(Arg::with_name("token").short("t").long("token").help(
            "Plex API token.",
        ))
        .arg(Arg::with_name("host").short("h").long("host").help(
            "Plex server endpoint.",
        ))
        .arg(Arg::with_name("mountpoint").index(1).required(true))
}

fn main() {
    let matches = app().get_matches();
    if matches.is_present("version") {
        println!("reaktor-mapper {}", crate_version!());
        return;
    }

    let host = matches.value_of("host")
        .unwrap_or("192.168.1.100:32400")
        .parse()
        .unwrap();
    let token = matches.value_of("token")
        .unwrap_or("")
        .into();
    let section = 10;
    let media_kind = api::MediaKind::Music;

    // {
    //     let api = api::PlexAPI::new(host, token);

    //     let container = api.metadata_children(20056).unwrap();
    //     println!("{:?}", container);
    // }

    let fs = fs::PlexFS::new(host, token, section, media_kind);

    env_logger::init();
    let mountpoint = env::args_os().nth(1).unwrap();
    let options = ["-d", "-o", "ro", "-o", "fsname=plex"]
        .iter()
        .map(|o| o.as_ref())
        .collect::<Vec<&OsStr>>();
    fuse::mount(fs, mountpoint, &options).unwrap();
}
