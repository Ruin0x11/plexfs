use std::cmp;
use std::collections::HashMap;
use std::ffi::{OsString, OsStr};
use std::net::SocketAddr;
use std::time::{Duration, UNIX_EPOCH};
use libc::ENOENT;
use fuse::{FileType, FileAttr, Filesystem, Request, ReplyData, ReplyEntry, ReplyAttr, ReplyDirectory};

use super::api;

const TTL: Duration = Duration::from_secs(60 * 60);

const PAGE_SIZE: u64 = 50;

struct Entry {
    rating_key: u64,
    kind: FileType,
    attr: Option<FileAttr>
}

pub struct PlexFS {
    api: api::PlexAPI,
    section: u64,
    kind: api::MediaKind,
    entries: HashMap<u64, HashMap<OsString, Entry>>,
}

impl PlexFS {
    pub fn new(host: SocketAddr, token: String, section: u64, kind: api::MediaKind) -> Self {
        PlexFS {
            api: api::PlexAPI::new(host, token),
            section: section,
            kind: kind,
            entries: HashMap::new()
        }
    }
}

const INO_ROOT: u64 = 1;

const ROOT_DIR_ATTR: FileAttr = FileAttr {
    ino: INO_ROOT,
    size: 0,
    blocks: 0,
    atime: UNIX_EPOCH,
    mtime: UNIX_EPOCH,
    ctime: UNIX_EPOCH,
    crtime: UNIX_EPOCH,
    kind: FileType::Directory,
    perm: 0o444,
    nlink: 2,
    uid: 501,
    gid: 20,
    rdev: 0,
    flags: 0,
};

fn to_attr(item: &api::Item) -> Option<FileAttr> {
    match item {
        api::Item::Directory {
            rating_key,
            last_viewed_at,
            updated_at,
            added_at,
            ..
        } => {
            let atime = UNIX_EPOCH + Duration::from_secs(*last_viewed_at);
            let mtime = UNIX_EPOCH + Duration::from_secs(*updated_at);
            let ctime = UNIX_EPOCH + Duration::from_secs(*added_at);
            let crtime = ctime;

            Some(FileAttr {
                ino: INO_ROOT + rating_key,
                size: 0,
                blocks: 0,
                atime: atime,
                mtime: mtime,
                ctime: ctime,
                crtime: crtime,
                kind: FileType::Directory,
                perm: 0o444,
                nlink: 1,
                uid: 501,
                gid: 20,
                rdev: 0,
                flags: 0,
            })
        },
        api::Item::Track {
            rating_key,
            last_viewed_at,
            updated_at,
            added_at,
            media,
            ..
        } => {
            let atime = UNIX_EPOCH + Duration::from_secs(*last_viewed_at);
            let mtime = UNIX_EPOCH + Duration::from_secs(*updated_at);
            let ctime = UNIX_EPOCH + Duration::from_secs(*added_at);
            let crtime = ctime;
            let size = media.part.size;

            Some(FileAttr {
                ino: INO_ROOT + rating_key,
                size: size,
                blocks: 1,
                atime: atime,
                mtime: mtime,
                ctime: ctime,
                crtime: crtime,
                kind: FileType::RegularFile,
                perm: 0o444,
                nlink: 1,
                uid: 501,
                gid: 20,
                rdev: 0,
                flags: 0,
            })
        },
        _ => None
    }
}

fn escape_name(s: &str) -> String {
    str::replace(s, "/", "_")
}

impl Filesystem for PlexFS {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        debug!("lookup {} {:?}", parent, name);

        match self.entries.get(&parent) {
            Some(names) => {
                match names.get(name) {
                    Some(entry) => match entry.attr {
                        Some(attr) => reply.entry(&TTL, &attr, 0),
                        None => reply.error(ENOENT)
                    }
                    _ => reply.error(ENOENT)
                }
            }
            None => reply.error(ENOENT)
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        debug!("getattr {}", ino);

        if ino == INO_ROOT {
            reply.attr(&TTL, &ROOT_DIR_ATTR);
            return
        }

        match self.api.metadata(ino - INO_ROOT) {
            Ok(container) => {
                match container.items.get(0) {
                    Some(item) => {
                        match to_attr(item) {
                            Some(attr) => reply.attr(&TTL, &attr),
                            None => reply.error(ENOENT)
                        }
                    }
                    None => reply.error(ENOENT)

                }
            },
            Err(_) => reply.error(ENOENT)
        }
    }

    fn read(&mut self, _req: &Request, ino: u64, _fh: u64, offset: i64, size: u32, reply: ReplyData) {
        debug!("read {} {} {}", ino, offset, size);

        if ino == INO_ROOT {
            reply.error(ENOENT);
            return
        }

        match self.api.metadata(ino - INO_ROOT) {
            Ok(container) => {
                match container.items.get(0) {
                    Some(item) => {
                        match item {
                            api::Item::Track { media, .. } => {
                                match self.api.file(&media.part, offset, size) {
                                    Ok(body) => reply.data(&body[0..cmp::min(size as usize, body.len())]),
                                    Err(_) => reply.error(ENOENT)
                                }
                            }
                            _ => reply.error(ENOENT)
                        }
                    }
                    None => reply.error(ENOENT)
                }
            },
            Err(_) => reply.error(ENOENT)
        }
    }

    fn readdir(&mut self, _req: &Request, ino: u64, _fh: u64, offset: i64, mut reply: ReplyDirectory) {
        debug!("readdir {} {}", ino, offset);

        if !self.entries.contains_key(&ino) {
            let mut en = HashMap::new();

            let mut containers = vec![];

            if ino == INO_ROOT {
                let mut start = 0;
                if let Ok((first, size)) = self.api.all(self.section, self.kind, start, PAGE_SIZE) {
                    containers.push(first);
                    start += PAGE_SIZE;
                    while start < size {
                        if let Ok((container, _)) = self.api.all(self.section, self.kind, start, PAGE_SIZE) {
                            containers.push(container);
                        }
                        start += PAGE_SIZE;
                    }
                }
            } else {
                let mut start = 0;
                if let Ok((first, size)) = self.api.metadata_children(ino - INO_ROOT, start, PAGE_SIZE) {
                    containers.push(first);
                    start += PAGE_SIZE;
                    while start < size {
                        if let Ok((container, _)) = self.api.metadata_children(ino - INO_ROOT, start, PAGE_SIZE) {
                            containers.push(container);
                        }
                        start += PAGE_SIZE;
                    }
                }
            }

            for container in containers.iter() {
                for item in container.items.iter() {
                    let attr = to_attr(&item);

                    match item {
                        api::Item::Directory { rating_key, title, .. } => {
                            en.insert(OsString::from(escape_name(title)), Entry {rating_key: *rating_key, kind: FileType::RegularFile, attr: attr});
                        },
                        api::Item::Track { rating_key, media, .. } => {
                            let path = &media.part.file;
                            let filename: String = path.split("/").last().unwrap().into();
                            en.insert(OsString::from(filename), Entry {rating_key: *rating_key, kind: FileType::RegularFile, attr: attr});
                        },
                        _ => ()
                    }
                }
            }

            self.entries.insert(ino, en);
        }

        let entries = self.entries.get(&ino).unwrap();

        for (i, (name, entry)) in entries.iter().enumerate().skip(offset as usize) {
            reply.add(INO_ROOT + entry.rating_key, (i + 1) as i64, entry.kind, name);
        }

        reply.ok();
    }
}
