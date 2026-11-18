//! The bridge reads the v0.3.1 light client's records (lez-v03/, branch testnet-v03, merged): the account it
//! guards and the bytes it decodes must be the ones that program writes. Its own wire crate is the reference.

use ferrata_lez03_bridge_core::{header_account, header_seed, LcRecord};
use ferrata_v03_wire as lc;
use lee_core::account::AccountId;

#[test]
fn the_bridge_reads_records_where_and_as_the_light_client_writes_them() {
    let program = AccountId::new([0x5a; 32]);
    for (i, slot) in [0u64, 1, 11_563_328, u64::MAX].into_iter().enumerate() {
        let instance = [i as u8 + 1; 32];
        assert_eq!(header_seed(&instance, slot), lc::header_seed(&instance, slot));
        assert_eq!(header_account(program, &instance, slot), lc::header_account(&program, &instance, slot));
        let theirs = lc::RecordState { instance_id: instance, chain_id: 11_155_111, record: vec![i as u8; 97] };
        let ours = LcRecord { instance_id: instance, chain_id: 11_155_111, record: vec![i as u8; 97] };
        let bytes = borsh::to_vec(&theirs).unwrap();
        assert_eq!(bytes, borsh::to_vec(&ours).unwrap());
        assert_eq!(borsh::from_slice::<LcRecord>(&bytes).unwrap(), ours);
    }
}
