# van-emde-boas-successor

A Rust van Emde Boas integer set for universes of 16, 256, and 65,536 keys.
The library supports insertion, removal, membership, strict predecessor and
successor, and cached minimum/maximum. It allocates the recursive hierarchy
up front: construction and storage are O(U), operations are O(log log U), and
minimum/maximum are O(1).

```rust
use veb_successor::VebSet;

let mut set = VebSet::new(256).unwrap();
for key in [3, 7, 42] { set.insert(key).unwrap(); }
assert_eq!(set.successor(7).unwrap(), Some(42));
assert_eq!(set.predecessor(7).unwrap(), Some(3));
set.validate().unwrap();
```

Run `cargo test` and `cargo clippy --all-targets -- -D warnings`.
