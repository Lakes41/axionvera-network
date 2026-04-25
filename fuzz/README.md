# Fuzzing Notes

This repository already contains `cargo-fuzz` infrastructure for `network-node` under `fuzz/`.

For the vault contract reward-index math, the repository now also uses `proptest` inside
`contracts/vault-contract` to hammer the `distribute_rewards` accounting path with:

- tiny `total_deposits` values such as `1`, `2`, and `3`
- extreme reward amounts including `i128::MAX`
- checked scaling by `REWARD_INDEX_SCALE` (`1e18`)

The goal of the vault fuzz coverage is to prove that reward-index math fails with a contract
error like `MathOverflow` or `ZeroRewardIncrement` instead of panicking on unchecked
multiplication or division.

Relevant coverage lives in:

- `contracts/vault-contract/src/lib.rs`
- `contracts/vault-contract/src/storage.rs`
