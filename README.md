This crate is something of a Frankenstein's monster of a crate, which I used for
a bunch of things.
1. To learn Tonic (this was used for the routeguide tutorial).
1. To test out my [active_standby](https://crates.io/crates/active_standby) crate.
1. To learn Rocket.
1. To deploy wasm without using node (failed at this...).

If you are interested in running things here are the different parts:

To run the backend server (doesn't have any state on startup):
```
cargo run --bin server
```

To update the server:
```
cargo run --bin route_programmer
```

To run the client from the command line:
```
cargo run --bin route_user
```

To run the client from the web:
```
cargo run --bin web_user
```