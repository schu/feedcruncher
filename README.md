# feedcruncher

![](https://github.com/schu/feedcruncher/workflows/feedcruncher-ci/badge.svg)

feedcruncher is a small daemon to watch RSS feeds and send notifications
for every new item. Supported notification targets are [Discord](https://support.discord.com/hc/en-us/articles/228383668-Intro-to-Webhooks)
and [Slack](https://api.slack.com/messaging/webhooks) webhooks.

## Requirements

* libsqlite3-dev

## Configuration

Example `feedcruncher.toml`:

```toml
# `pool` enables / disables polling, the default is enabled polling.
# With disabled polling feedcruncher can be run as a "one-shot" job.
# (optional)
poll = true

# `poll_sleep_dur` defines the time to sleep in seconds between polling,
# the default is 600 seconds.
# (optional)
poll_sleep_dur = 600

# `db_path` can be used to set a custom database path, the default is
# `sqlite://./feedcruncher.sqlite3`
# (optional)
db_path = "sqlite://./feedcruncher.sqlite3"

# `webhook_urls` defines a list of webhook urls and can be set per
# feed as well as globally. `-` can be set to make feedcruncher print
# feed items to stdout.
# (required)
webhook_urls = [
  "-",
  "https://discordapp.com/api/webhooks/..."
]

# `feeds` is a list of feeds to poll ("array of tables" in TOML).
# 
# `kind`         (required) defines the kind of feed – "rss" or "atom"
# `url`          (required) is the feed URL to poll
# `webhook_urls` (optional) defines a list of webhooks to be used instead
#                           of the global default
[[feeds]]
kind = "rss"
url = "https://www.schu.io/index.xml"

[[feeds]]
kind = "atom"
url = "https://blog.rust-lang.org/feed.xml"
webhook_urls = [
  "https://hooks.slack.com/..."
]

# More feeds ...
```

## Usage

```
feedcruncher --config feedcruncher.toml
```

## Development

### sqlx

The [sqlx-cli](https://github.com/launchbadge/sqlx/blob/main/sqlx-cli/README.md)
is required to add/run migrations and to "enable building in offline mode" with
prepare.

```
cargo sqlx database create

cargo sqlx migrate add
cargo sqlx migrate run

cargo sqlx prepare
```

### Testing

Run the test feedserver from one terminal:

```
$ make run-server
    Finished dev [unoptimized + debuginfo] target(s) in 0.13s
     Running `target/debug/feedserver`
Press Return to add new feed item ...
```

Run feedcruncher from a second terminal:

```
$ make run
    Finished dev [unoptimized + debuginfo] target(s) in 0.14s
     Running `target/debug/feedcruncher --config config-test.toml`
Watching [
    FeedConfig {
        url: "http://localhost:4321",
    },
]
Waiting for new feed items ...
```

## Roadmap

* [ ] Add Matrix as notification target
* [ ] Add a minimalistic web view
* [ ] Add a syndicated feed endpoint

## License

[AGPL v3](https://www.gnu.org/licenses/agpl-3.0.en.html)
