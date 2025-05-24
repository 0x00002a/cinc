# Cinc
_We have steam cloud at home_

## Introduction

This is a tool designed to give a similar experience to steam cloud sync for games which don't have it
on steam. It's goals are:

- To be as low configuration as possible
- To be as easy as possible to "fire and forget" you should set it up once for each computer and game and forget about it
- To not introduce noticeable overhead to the startup time of games
- Try our very best not to cause unintentional data loss

## Installation

`cargo install cinc`

## Usage

In order to actually sync to a shared location and not just the local filesystem you will need to configure
cinc to use a backend. Currently the only supported backend of this nature is WebDav. You can add a WebDav
backend with the following command:

```bash
cinc backends add \
    --name=<splung> \
    --ty=webdav \
    --root=/cinc \
    --webdav-url=<url> \
    --webdav-username=<username> \
    --webdav-psk=<psk> \
    --set-default
```

The `--webdav-psk` and `--root` arguments are optional, but the psk required if
your webdav server needs authentication and the root is recommended unless you
include it in the url or are okay with cinc using the root of the WebDav
server. The rest of the arguments are required, you can learn more about what
they do with `cinc backends --help`.

Once you have configured a backend you can move on to actually using cinc,
currently the only supported platform is steam.

### Steam

Set the launch options field to:

```bash
cinc launch -- %command%
```

## Supported operating systems

- Linux
- Probably windows but I haven't tested it (works with wine/proton games on Linux though)
- Theoretically Macs should work but again untested

## Support for games

Technically not _all_ games are supported. Cinc uses the
[ludusavi](https://github.com/mtkennerly/ludusavi) manifest format (and
ludusavi's manifest by default) and I have yet to find a game that _isn't_ on
that list, but if a game isn't supported and you want it recommend either changing
the `default_manifest` url in the config to point at your custom manifest or contributing
to the ludusavi one directly.

Note that the manifest is cached so if you do this you will need to update it with

```bash
cinc --update
```
