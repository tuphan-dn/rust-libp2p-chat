# rust-libp2p-chat

To connect the boostrap node:

```
cargo run -- --bootstrap /dns/<name>.fly.dev/tcp/8080/p2p/<key> --silent
```

For example,

```
cargo run -- --bootstrap /dns/p2p-bootstrap.fly.dev/tcp/8080/p2p/12D3KooWG8YCUWuu86jtyF3Dbe7Uc7wChR98EYAKzPcC4VK2Y8jy --silent
```

Bootstrap table

| Domain        | Key                                                  |
| ------------- | ---------------------------------------------------- |
| p2p-bootstrap | 12D3KooWG8YCUWuu86jtyF3Dbe7Uc7wChR98EYAKzPcC4VK2Y8jy |
| master-1      | TBD                                                  |
| master-2      | TBD                                                  |
