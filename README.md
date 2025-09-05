# Antube

Download files from the Autonomi Network — one click to eternal access.

Censorship-proof, universally available, and free for everyone.

Download files that were uploaded to the network forever.

Liberate the world's knowledge — access it from anywhere

## Download it

[Download the latest release on github with a click!](https://github.com/maidsafe/antube/releases/latest) 

> Mac users might face quarantine issues: `"Antube.app" is damaged and can't be opened. You should move it to the Trash.`
>
> This happens because we don't have a $99 a year Apple Developer account :(
>
> To fix this:
> 1. **Unzip** the file (double-click the `.zip`).
> 2. Open **Terminal** (press `Cmd + Space`, type "Terminal", and press Enter).
> 3. Go to your Downloads folder:
>   ```bash
>   cd ~/Downloads
>   ```
> 4. Remove macOS quarantine flag:
>   ```bash
>   xattr -rd com.apple.quarantine Antube.app
>   ```
> 5. Double-click **Antube.app** to open it!

## Build it from source

```bash
# clone the autonomi repo
git clone https://github.com/maidsafe/autonomi.git 
cd autonomi
git checkout main

# go back into the antube directory
cd ../antube

# build the release version of the app
cargo build --release

# (for macOS) make a Antube.app
bash ./assets/mac_os_bundle.sh
```

## Run it from source

```bash
cargo run --release
```

## For those diving into the code

- The `src/server.rs` file contains the main logic for all autonomi network interaction
- The `src/main.rs` contains the GUI front-end for the app

## Features

- Download files from the Autonomi Network using file addresses
- Simple drag-and-drop interface
- Cross-platform support (Linux, macOS)
- Free downloads forever

## Coming soon

- Enhanced file management
- Download history
- Progress indicators
- Suggest more features by submitting or upvoting an issue on github