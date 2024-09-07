# Verfassungsbooks Rendering Server

This is the rendering server software for verfassungsbooks servers.

# Installation
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

## Using prebuilt amd64 binary:
Go to the [latest build](https://builds.sr.ht/~verfassungsblog/Verfassungsbooks-Rendering-Server/commits/master) and download the verfassungsbooks-rendering-server-bundled.tar.gz.

## Running the server
### Dependencies
* Install the dependencies for chromium (ubuntu example): `apt install bubblewrap libnss3-tools libatk-bridge2.0-0 libcups2 libxcomposite-dev libxrandr2 libxdamage1 libasound2t64 libcairo2 libasound2t64 libgbm1 libpango-1.0-0`
### Rendering Env
Either run setup.sh inside rendering-envs (which will take a few hours), or download the prebuilt environment [here](https://builds.sr.ht/~verfassungsblog/vb-rendering-envs) (open the latest success build and download the artifact.
### Configuration
Copy the default config config/default.toml to config/local.toml and change if necessary. Copy the mtls certificates to an appropriate location and set paths in config.
See the verfassungsbooks repository for hints for CA & Certificate creation.
