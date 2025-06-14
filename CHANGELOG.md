# 0.2.3

- Fix panic on upload if webdav root does not start with a slash
- Fix error on upload after the first if root was more than one level deep

# 0.2.2

- Check for incompatible cinc version on the server
- Fix unable to just pass `--update`

# 0.2.1

- Able to run under pure wine as well as umu
- Able to find more games automatically when run under heroic
- Fix missing template variable for some games under heroic

# 0.2.0

- General terminal UX improvements
- Support for non-steam games through steam
- Support for system keychain for storing passwords
- Fix dialog windows staying open even after game launch (eframe bug, switched to popout)
- Fix manifest url config option being ignored
- Add config validation
- Fix downloading files from cloud when unneeded (on every launch)
- Fix syncing of config files
- Support for umu games and thus non-steam launchers (e.g. heroic)
