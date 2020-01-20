use std::cmp;
use std::ffi::OsStr;
use std::net::SocketAddr;
use std::time::{Duration, UNIX_EPOCH};
use libc::ENOENT;
use fuse::{FileType, FileAttr, Filesystem, Request, ReplyData, ReplyEntry, ReplyAttr, ReplyDirectory};

use super::api;

const TTL: Duration = Duration::from_secs(1);           // 1 second

pub struct PlexFS {
    api: api::PlexAPI,
    section: u64,
    kind: api::MediaKind
}

impl PlexFS {
    pub fn new(host: SocketAddr, token: String, section: u64, kind: api::MediaKind) -> Self {
        PlexFS {
            api: api::PlexAPI::new(host, token),
            section: section,
            kind: kind
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
    perm: 0o755,
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
                perm: 0o644,
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
            println!("SIZE {}", size);

            Some(FileAttr {
                ino: INO_ROOT + rating_key,
                size: size,
                blocks: 1,
                atime: atime,
                mtime: mtime,
                ctime: ctime,
                crtime: crtime,
                kind: FileType::RegularFile,
                perm: 0o555,
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
        info!("lookup {} {:?}", parent, name);

        let res = if parent == INO_ROOT {
            self.api.all(self.section, self.kind)
        } else {
            self.api.metadata_children(parent - INO_ROOT)
        };

        match res {
            Ok(container) => {
                for item in container.items.iter() {
                    let title = match item {
                        api::Item::Directory { title, .. } => escape_name(title),
                        api::Item::Video { title, .. } => escape_name(title),
                        api::Item::Track { media, .. } => {
                            let path = &media.part.file;
                            path.split("/").last().unwrap().into()
                        },
                    };
                    if name.to_str() == Some(&title) {
                        println!("get {:?}", item);
                        match to_attr(item) {
                            Some(attr) => reply.entry(&TTL, &attr, 0),
                            None => reply.error(ENOENT)
                        }
                        return
                    }
                }
                reply.error(ENOENT);
            },
            Err(_) => reply.error(ENOENT)
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        info!("getattr {}", ino);

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
        info!("read {} {} {}", ino, offset, size);

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
                                println!("getfile");
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
        info!("readdir {} {}", ino, offset);

        let mut entries = vec![
            (1, FileType::Directory, String::from(".")),
            (1, FileType::Directory, String::from("..")),
        ];

        let res = if ino == INO_ROOT {
            self.api.all(self.section, self.kind)
        } else {
            self.api.metadata_children(ino - INO_ROOT)
        };

        if let Ok(container) = res {
            for item in container.items.iter() {
                match item {
                    api::Item::Directory { rating_key, title, .. } => {
                        entries.push((INO_ROOT + rating_key, FileType::Directory, escape_name(title)))
                    },
                    api::Item::Track { rating_key, media, .. } => {
                        let path = &media.part.file;
                        let filename = path.split("/").last().unwrap().into();
                        entries.push((INO_ROOT + rating_key, FileType::RegularFile, filename))
                    }
                    _ => ()
                }
            }
        }

        for (i, entry) in entries.into_iter().enumerate().skip(offset as usize) {
            // i + 1 means the index of the next entry
            reply.add(entry.0, (i + 1) as i64, entry.1, &entry.2);
        }
        reply.ok();
    }
}
