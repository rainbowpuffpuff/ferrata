//! Prints the account ids of LEZ v0.4.0-rc1's published test accounts (`testnet_initial_state`,
//! funded by the genesis of `local.json`), as the hex the bridge core's `dest` takes. The inbound
//! run's new deposits name them as their LEZ destinations (`lez/live/inbound/deposits.py`).
fn main() {
    for (i, a) in testnet_initial_state::initial_pub_accounts_private_keys().iter().enumerate() {
        println!("public {i} {}", hex::encode(a.account_id.value()));
    }
    for (i, a) in testnet_initial_state::initial_priv_accounts_private_keys().iter().enumerate() {
        println!("private {i} {}", hex::encode(a.account_id().value()));
    }
}
