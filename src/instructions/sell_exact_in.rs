use carbon_raydium_launchpad_decoder::{
    instructions::{
        buy_exact_in::BuyExactIn,
        sell_exact_in::{SellExactIn, SellExactInInstructionAccounts},
    },
    PROGRAM_ID as LAUNCHPAD_PROGRAM_ID,
};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

pub trait SellExactInInstructionAccountsExt {
    fn get_buy_ix(&self, buy_params: BuyExactIn) -> Instruction;
    fn get_sell_ix(&self, sell_params: SellExactIn) -> Instruction;
    fn get_create_idempotent_ata_ix(&self) -> Vec<Instruction>; 
    fn get_create_ata_ix(&self) -> Instruction;
}

impl SellExactInInstructionAccountsExt for SellExactInInstructionAccounts {
    fn get_create_ata_ix(&self) -> Instruction {
        let create_ata_ix =
            spl_associated_token_account::instruction::create_associated_token_account(
                &self.payer,
                &self.payer,
                &self.base_token_mint,
                &self.base_token_program,
            );

        create_ata_ix
    }

    fn get_create_idempotent_ata_ix(&self) -> Vec<Instruction> {
        let create_ata_ix1 =
            spl_associated_token_account::instruction::create_associated_token_account_idempotent(
                &self.payer,
                &self.payer,
                &self.base_token_mint,
                &self.base_token_program,
            );

        let create_ata_ix2 =
            spl_associated_token_account::instruction::create_associated_token_account_idempotent(
                &self.payer,
                &self.payer,
                &self.quote_token_mint,
                &self.quote_token_program,
            );

        vec![create_ata_ix1, create_ata_ix2]
    }

    fn get_buy_ix(&self, buy_params: BuyExactIn) -> Instruction {
        let discriminator = [250, 234, 13, 123, 213, 156, 19, 236];
        let mut data = Vec::new();

        data.extend_from_slice(&discriminator);
        data.extend_from_slice(&buy_params.amount_in.to_le_bytes());
        data.extend_from_slice(&buy_params.minimum_amount_out.to_le_bytes());
        data.extend_from_slice(&buy_params.share_fee_rate.to_le_bytes());

        let system_program = Pubkey::from_str_const("11111111111111111111111111111111");
        let remaining_account1 = &Pubkey::from_str_const("Cyu7XFTGSHSwFtsghriq9DfGVMehrdCaepFefFKNdcKB");
        let remaining_account2 = &Pubkey::from_str_const("3togC4WnVohRh4QYqGVB5VLdz3VwX2RKTgybM8VZcwUd");

        // Then encode the struct fields using Borsh
        let accounts = vec![
            AccountMeta::new(self.payer, true),                    // #1 - Payer (Signer, Writable, Fee Payer)
            AccountMeta::new_readonly(self.authority, false), // #2 - authority
            AccountMeta::new_readonly(self.global_config, false), // #3 - Global Config
            AccountMeta::new_readonly(self.platform_config, false), // #4 - Platform Config
            AccountMeta::new(self.pool_state, false), // #5 - Pool State
            AccountMeta::new(self.user_base_token, false), // #6 - User Base Token Account
            AccountMeta::new(self.user_quote_token, false), // #7 - User Quote Token Account
            AccountMeta::new(self.base_vault, false), // #8 - Pool Base Token Account
            AccountMeta::new(self.quote_vault, false), // #9 - Pool Quote Token Account
            AccountMeta::new_readonly(self.base_token_mint, false), // #10 - Base Mint (usd1)
            AccountMeta::new_readonly(self.quote_token_mint, false), // #11 - Quote Mint (other token)
            AccountMeta::new_readonly(self.base_token_program, false), // #12 - Base Token Program (Token Program)
            AccountMeta::new_readonly(self.quote_token_program, false), // #13 - Quote Token Program (Token Program)
            AccountMeta::new_readonly(self.event_authority, false), // #14 - Event Authority
            AccountMeta::new_readonly(self.program, false), // #15 - Program (Launchpad)
            AccountMeta::new_readonly(system_program, false), // #16 - System Program
            AccountMeta::new_readonly(*remaining_account1, false), // #17 - Remaining Account 1
            AccountMeta::new_readonly(*remaining_account2, false), // #18 - Remaining Account 2
        ];

        Instruction {
            program_id: LAUNCHPAD_PROGRAM_ID,
            accounts,
            data,
        }
    }

    fn get_sell_ix(&self, sell_params: SellExactIn) -> Instruction {
        let discriminator = [149, 39, 222, 155, 211, 124, 152, 26];
        let mut data = Vec::new();

        data.extend_from_slice(&discriminator);
        data.extend_from_slice(&sell_params.amount_in.to_le_bytes());
        data.extend_from_slice(&sell_params.minimum_amount_out.to_le_bytes());
        data.extend_from_slice(&sell_params.share_fee_rate.to_le_bytes());

        let system_program = Pubkey::from_str_const("11111111111111111111111111111111");
        let remaining_account1 = &Pubkey::from_str_const("Cyu7XFTGSHSwFtsghriq9DfGVMehrdCaepFefFKNdcKB");
        let remaining_account2 = &Pubkey::from_str_const("3togC4WnVohRh4QYqGVB5VLdz3VwX2RKTgybM8VZcwUd");

        // Then encode the struct fields using Borsh
        let accounts = vec![
            AccountMeta::new(self.payer, true),                    // #1 - Payer (Signer, Writable, Fee Payer)
            AccountMeta::new_readonly(self.authority, false), // #2 - authority
            AccountMeta::new_readonly(self.global_config, false), // #3 - Global Config
            AccountMeta::new_readonly(self.platform_config, false), // #4 - Platform Config
            AccountMeta::new(self.pool_state, false), // #5 - Pool State
            AccountMeta::new(self.user_base_token, false), // #6 - User Base Token Account
            AccountMeta::new(self.user_quote_token, false), // #7 - User Quote Token Account
            AccountMeta::new(self.base_vault, false), // #8 - Pool Base Token Account
            AccountMeta::new(self.quote_vault, false), // #9 - Pool Quote Token Account
            AccountMeta::new_readonly(self.base_token_mint, false), // #10 - Base Mint (usd1)
            AccountMeta::new_readonly(self.quote_token_mint, false), // #11 - Quote Mint (other token)
            AccountMeta::new_readonly(self.base_token_program, false), // #12 - Base Token Program (Token Program)
            AccountMeta::new_readonly(self.quote_token_program, false), // #13 - Quote Token Program (Token Program)
            AccountMeta::new_readonly(self.event_authority, false), // #14 - Event Authority
            AccountMeta::new_readonly(self.program, false), // #15 - Program (Launchpad)
            AccountMeta::new_readonly(system_program, false), // #16 - System Program
            AccountMeta::new_readonly(*remaining_account1, false), // #17 - Remaining Account 1
            AccountMeta::new_readonly(*remaining_account2, false), // #18 - Remaining Account 2
        ];

        Instruction {
            program_id: LAUNCHPAD_PROGRAM_ID,
            accounts,
            data,
        }
    }
}
