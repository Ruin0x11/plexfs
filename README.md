# plexfs
Mount your Plex Library as a local filesystem using FUSE.

*Note:* Only works for music currently.

## Usage
1. Obtain an `X-Plex-Token`. See [here](https://support.plex.tv/articles/204059436-finding-an-authentication-token-x-plex-token/).
2. Determine which library to use. Open the library from the sidebar in the Plex web app and look for `sections` in the URL.

```
http://192.168.1.100:32400/web/index.html#!/media/6e3210dcc21650fc7f197c740face0521e3a9ba4/com.plexapp.plugins.library?key=%2Flibrary%2Fsections%2F10%2Fall%3Ftype%3D8&pageType=list&context=content.library&source=%2Fhubs%2Fsections%2F10
```

In this case the `section` is 10.

3. Run the following.

```
cargo run -- --token=<X-Plex-Token> --host=192.168.1.100:32400 --section=10 ./mountpoint
```
