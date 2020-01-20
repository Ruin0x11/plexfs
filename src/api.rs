use std::io::Read;
use std::net::SocketAddr;

use reqwest;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, RANGE};
use anyhow::Result;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use quick_xml::de::from_str;

pub struct PlexAPI {
    host: SocketAddr,
    token: String
}

#[derive(Debug, Clone, Copy)]
pub enum MediaKind {
    Video = 1,
    TV = 2,
    Music = 8,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct MediaContainer {
    #[serde(rename="$value")]
    pub items: Vec<Item>
}

#[derive(Deserialize, PartialEq, Debug)]
pub enum Item {
    Directory {
        #[serde(rename="ratingKey", default)]
        rating_key: u64,
        guid: String,
        title: String,
        #[serde(rename="parentTitle", default)]
        parent_title: String,
        summary: String,
        #[serde(rename="lastViewedAt", default)]
        last_viewed_at: u64,
        #[serde(rename="addedAt", default)]
        added_at: u64,
        #[serde(rename="updatedAt", default)]
        updated_at: u64,
    },
    Video {
        title: String,
        #[serde(rename="grandparentTitle", default)]
        grandparent_title: String,
        #[serde(rename="Media", default)]
        media: Media
    },
    Track {
        #[serde(rename="ratingKey", default)]
        rating_key: u64,
        guid: String,
        title: String,
        #[serde(rename="parentTitle", default)]
        parent_title: String,
        summary: String,
        #[serde(rename="lastViewedAt", default)]
        last_viewed_at: u64,
        #[serde(rename="addedAt", default)]
        added_at: u64,
        #[serde(rename="updatedAt", default)]
        updated_at: u64,
        #[serde(rename="Media", default)]
        media: Media
    }
}

#[derive(Deserialize, PartialEq, Debug)]
pub struct Media {
    pub container: Option<String>,
    #[serde(rename="videoResolution", default)]
    pub video_resolution: Option<String>,
    pub duration: u64,
    #[serde(rename="Part", default)]
    pub part: Part
}

impl Default for Media {
    fn default() -> Self {
        Media {
            container: None,
            video_resolution: None,
            duration: 0,
            part: Part::default()
        }
    }
}

#[derive(Deserialize, PartialEq, Debug)]
pub struct Part {
    pub key: String,
    pub file: String,
    pub size: u64,
    pub container: Option<String>,
}

impl Default for Part {
    fn default() -> Self {
        Part {
            key: String::new(),
            file: String::new(),
            size: 0,
            container: None,
        }
    }
}

impl PlexAPI {
    pub fn new(host: SocketAddr, token: String) -> Self {
        PlexAPI {
            host: host,
            token: token
        }
    }

    fn get_paged<T>(&self, url: &str, args: &str, start: u64, size: u64) -> Result<(T, u64)>
        where T: DeserializeOwned
    {
        let args = format!("{}&X-Plex-Container-Start={}&X-Plex-Container-Size={}", args, start, size);
        let full_url = format!("http://{}{}?X-Plex-Token={}{}", self.host, url, self.token, args);
        let resp = reqwest::blocking::get(&full_url)?;
        debug!("GET {}", full_url);
        let header_name = HeaderName::from_static("x-plex-container-total-size");
        let page_size = resp.headers()
            .get(header_name)
            .map(|h| h.to_str().unwrap().parse::<u64>())
            .unwrap_or(Ok(0))?;
        let result = from_str(&resp.text()?)?;
        Ok((result, page_size))
    }

    fn get<T>(&self, url: &str, args: &str) -> Result<T>
        where T: DeserializeOwned
    {
        self.get_paged(url, args, 0, 100).map(|(resp, _)| resp)
    }

    pub fn recently_added(&self, kind: MediaKind) -> Result<MediaContainer> {
        let args = format!("&type={}", kind as u8);
        self.get("/hubs/home/recentlyAdded", &args)
    }

    pub fn all(&self, section: u64, kind: MediaKind, start: u64, size: u64) -> Result<(MediaContainer, u64)> {
        let url = format!("/library/sections/{}/all", section);
        let args = format!("&type={}", kind as u8);
        self.get_paged(&url, &args, start, size)
    }

    pub fn metadata(&self, rating_key: u64) -> Result<MediaContainer> {
        let url = format!("/library/metadata/{}", rating_key);
        self.get(&url, "")
    }

    pub fn metadata_children(&self, rating_key: u64, start: u64, size: u64) -> Result<(MediaContainer, u64)> {
        let url = format!("/library/metadata/{}/children", rating_key);
        self.get_paged(&url, "&excludeAllLeaves=1&includeExternalMedia=1", start, size)
    }

    pub fn file(&self, part: &Part, offset: i64, size: u32) -> Result<Vec<u8>> {
        let full_url = format!("http://{}{}?X-Plex-Token={}&X-Plex-Container-Start=0&X-Plex-Container-Size=100",
                          self.host, part.key, self.token);
        debug!("GET {}", full_url);
        let range = format!("bytes={}-{}", offset, offset + size as i64);
        let client = reqwest::blocking::Client::new();
        let mut headers = HeaderMap::new();
        headers.insert(RANGE, HeaderValue::from_str(&range).unwrap());
        let mut resp = client.get(&full_url)
            .headers(headers)
            .send()?;
        let mut buf = vec![];
        resp.read_to_end(&mut buf)?;
        Ok(buf)
    }
}
