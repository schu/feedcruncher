# feedcruncher

![](https://github.com/schu/feedcruncher/workflows/feedcruncher-ci/badge.svg)

Status: work in progress

feedcruncher is a small daemon to watch RSS feeds and send notifications for every new item.
Supported notification targets are Discord and Slack webhooks.

## Configuration

Example `feedcruncher.toml`:

```
sleep_dur: 300
webhook_url: "https://discordapp.com/api/webhooks/..."

[[feeds]]
url = "https://schu.io/index.xml"

[[feeds]]
url = "https://blog.rust-lang.org/feed.xml"
webhook_url = "https://hooks.slack.com/....."
```

`sleep_dur` defines the time to sleep in seconds between polling. Default: `600`

`webhook_url` defines the webhook url and can be set per feed as well as globally.

## Usage

```
feedcruncher --config feedcruncher.toml
```

## Testing

Start feedserver from first terminal:

```
$ cargo run --bin feedserver
    Finished dev [unoptimized + debuginfo] target(s) in 0.13s
     Running `target/debug/feedserver`
Press Return to add new feed item ...
```

Start feedcruncher from second terminal:

```
$ cargo run --bin feedcruncher -- --config config-test.toml
    Finished dev [unoptimized + debuginfo] target(s) in 0.14s
     Running `target/debug/feedcruncher --config test-config.toml`
Watching [
    FeedConfig {
        url: "http://localhost:4321",
    },
]
Waiting for new feed items ...
```

## License

[AGPL v3](https://www.gnu.org/licenses/agpl-3.0.en.html)
