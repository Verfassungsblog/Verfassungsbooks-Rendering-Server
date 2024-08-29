# Verfassungsbooks Rendering Server

This is the rendering server software for verfassungsbooks servers.

## Building from source
### Debian Example:
1. Install git & openssl & gcc & gcc-multilib:
`apt install git openssl gcc gcc-multilib`
2. Create a unprivileged user:
`adduser verfassungsbooks`
3. Log in as new user:
`su verfassungsbooks`
4. Install [rustup and cargo](https://rustup.rs/):
   `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
5. Relogin:
`exit`
`su verfassungsbooks`
7. cd to home:
`cd ~`
6. Clone respository:
`git clone https://git.sr.ht/~verfassungsblog/Verfassungsbooks-Rendering-Server`
7. cd to new directory:
`cd Verfassungsbooks-Rendering-Server/`
8. Build project:
`cargo build --release`

## Note on rendering environments
Currently the server requires rendering environments for pandoc & vivliostyle.
Since we want as few dependencies as possible, we need a static compiled version of
node & a chromium executable. You can just run the setup.sh inside
rendering-envs, but this will build it from scratch which may take multiple hours.

You probably want to use the prebuilt rendering-environments available [here]().
If you are using the 