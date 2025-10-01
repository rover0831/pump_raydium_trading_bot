use solana_sdk::{
    pubkey::Pubkey,
    hash::Hash,
    instruction::Instruction,
    message::{VersionedMessage, v0::Message},
    transaction::VersionedTransaction,
    signer::keypair::Keypair,
};   

pub fn build_and_sign(
    mut ixs: Vec<Instruction>,
    recent_blockhash: Hash,
    nonce_ix: Option<Instruction>,
    pubkey: Pubkey,
    keypair: Keypair,
) -> String {
    // If there's a nonce instruction, insert it at the start of the instruction list
    if let Some(nonce_instruction) = nonce_ix {
        ixs.insert(0, nonce_instruction);
    }

    let message = Message::try_compile(&pubkey, &ixs, &[], recent_blockhash)
        .expect("Failed to compile message");
    let versioned_message = VersionedMessage::V0(message);
    let txn = VersionedTransaction::try_new(versioned_message, &[&keypair])
        .expect("Failed to create transaction");

    let serialized_tx = bincode::serialize(&txn).expect("Failed to serialize transaction");

    base64::encode(&serialized_tx)
}
