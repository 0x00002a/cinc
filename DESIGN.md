# Design

## Problem

Some games do not have steam sync support, this is annoying. Some games are non steam and cannot have
steam sync support, this is annoying.

## Why not use rclone

Don't want to have to write a script for every game, should be automatic with as minimal config
as possible. Ideally an autodetector with config helper

## Must-haves

- Detect when application is running and sync it
- Multiple backends (to start maybe just nextcloud and filesystem (for testing))
