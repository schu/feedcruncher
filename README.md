# feedcruncher

![](https://github.com/schu/feedcruncher/workflows/feedcruncher-ci/badge.svg)

Status: work in progress

feedcruncher is a small daemon to watch RSS feeds and send push notifications
for new items.

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
$ cargo run --bin feedcruncher -- --sleep-dur 5 --webhook-url - http://localhost:4321
    Finished dev [unoptimized + debuginfo] target(s) in 0.13s
     Running `target/debug/feedcruncher --sleep-dur 5 --webhook-url - 'http://localhost:4321'`
Watching [
    "http://localhost:4321",
]
Waiting for new feed items ...
```

## License

[AGPL v3](https://www.gnu.org/licenses/agpl-3.0.en.html)
