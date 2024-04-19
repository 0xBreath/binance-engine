<p align="center">
  <a href="https://lunarengine.xyz">
    <img alt="Lunar Engine" src="./logo.png" width="250" />
  </a>
</p>

[//]: # (# Lunar Engine)

### Create Environment Variables
```bash
BINANCE_TEST_API_KEY=something
BINANCE_TEST_API_SECRET=something
BINANCE_LIVE_API_KEY=something
BINANCE_LIVE_API_SECRET=something
# true if using testnet Binance or Alpaca, false if live (real money!)
TESTNET=true
# true if want to read data on live network but not trade
DISABLE_TRADING=false
```

### Create Binance Test API Key
[Binance Test Login](https://testnet.binance.vision/)
See top of page "Log In with GitHub" to create an API key.


### Setup Heroku
* Configure an app with one `worker` dyno.
* Ensure the dynos are scaled to at least 1 worker dyno for the `Standard-2X` type 
for enough CPU to multithread tokio tasks.
* Install Rust buildpack: [link](https://github.com/emk/heroku-buildpack-rust)
* Check all environment variables are set in "Config Vars" section under Settings.

To follow logs:
```shell
# normal heroku logs
heroku logs -u -a lunar-engine

# from Logtail UI
heroku addons:open logtail -a lunar-engine
```


### Setup Google Cloud VM
```bash
# switch to root
sudo 

# GitHub, manage terminal processes, and Cargo build dependencies
sudo apt install -y git screen build-essential libsasl2-dev pkg-config libssl-dev libfontconfig1 libfontconfig1-dev

# Install Rust
curl https://sh.rustup.rs -sSf | sh
# set PATH to include cargo
. "$HOME/.cargo/env"

cd /var/lib
mkdir data
cd data

# Set GitHub remote
git init
git remote add origin https://github.com/cosmic-lab-inc/lunar-engine.git
git reset --hard origin/main
git pull origin main

# create ENV
touch .env
nano .env
# fill with ENV variables...

# Create a screen to run the algorithm
screen -R dreamrunner

# Start the algorithm on Binance
cargo run -r -p dreamrunner

# Exit screen with Ctrl+A then D

# Print logs on the main screen
cat dreamrunner.log
# Follow logs on the main screen
tail -f dreamrunner.log

# To reenter the screen
screen -r dreamrunner

# To kill the screen
screen -X -S dreamrunner quit
```

### Create Release Tag
Create a release tag and push to GitHub:
```bash
git tag -a tag-name -m 'tag-message'

git push origin tag-name
```
Go to GitHub and publish a release.