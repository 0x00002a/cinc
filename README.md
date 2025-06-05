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
    --set-default
```

The `--root` argument is optional, but is recommended unless you include it in
the url or are okay with cinc using the root of the WebDav server. You can
learn more about what each of these arguments does with `cinc backends add --help`.

Once you have configured a backend you can move on to actually using cinc on
your platform of choice. Currently it supports steam and anything that uses umu
or wine _in theory_ though for many games it will struggle to find the manifest
without being launched through steam or heroic. You can use the `--steam-app-id`
argument on `cinc launch` to solve this problem.

### Steam

Set the launch options field to:

```bash
cinc launch -- %command%
```

This might not work since cargo installs packages to a directory that isn't
always in the path by default, in that case use the absolute path for cinc
instead (you can get it by doing `which cinc` in your shell)

#### Launching non-steam games via steam

Launching non-steam games through steam has an added complication due to the
app id that steam assigns it not matching the actual steam app id for the game.
This means that `cinc` will not be able to automatically work out which game it
is launching. This is solved by passing the `--steam-app-id` argument with the
real steam app id (which can be found on [steamdb] or by examining the numeric
part of the store page url, e.g. for
<https://store.steampowered.com/app/1903340/Clair_Obscur_Expedition_33/> the
steam app id is `1903340`)

e.g. if you have bought clair obscur through another store front but want to run
it as a non-steam game through steam you would use the following command:

```bash
cinc launch --steam-app-id=1903340 -- %command%
```

### Heroic

To use cinc with the heroic launcher you must specify it as a wrapper in the
advanced options for the game. Set the wrapper path to the absolute path to the
binary (likely `/home/<name>/.local/share/cargo/bin/cinc`) and the arguments to
`launch`.

## A note on gamescope

If you want to run the game under gamescope or anything else which would
usually act as a wrapper make sure cinc is the **last** executable in the pile.
It depends on being passed the exact command to run the game itself (i.e. NOT
gamescope running the game) in order to do various parts of its platform
and sync paths detection.

e.g. to run a game under steam with gamescope and cinc:

```bash
gamescope -- cinc launch -- %command%
```

## Supported operating systems

- Linux
- Probably windows but I haven't tested it (works with wine/proton games on Linux though)
- Theoretically Macs should work but again untested

## Support for games

Technically not _all_ games are supported. cinc uses the
[ludusavi](https://github.com/mtkennerly/ludusavi) manifest format (and
ludusavi's manifest by default) and I have yet to find a game that _isn't_ on
that list, but if a game isn't supported and you want it recommend either changing
the `default_manifest` url in the config to point at your custom manifest or contributing
to the ludusavi one directly (which you should do by contribing to [pcgamingwiki]).

Note that the manifest is cached so if you do this you will need to update it with

```bash
cinc --update
```

[steamdb]: https://steamdb.info/
[pcgamingwiki]: https://www.pcgamingwiki.com/
