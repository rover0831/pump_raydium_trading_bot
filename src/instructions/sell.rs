use carbon_pump_swap_decoder::{
    instructions::{
        buy::Buy,
        sell::{Sell, SellInstructionAccounts},
    },
    PROGRAM_ID as PUMPSWAP_PROGRAM_ID,
};
use solana_program::example_mocks::solana_sdk::system_instruction;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};
use spl_associated_token_account::get_associated_token_address;
use spl_token::instruction::sync_native;

use crate::utils::blockhash::WSOL;

pub trait SellInstructionAccountsExt {
    fn get_buy_ix(&self, buy_params: Buy) -> Instruction;
    fn get_sell_ix(&self, sell_params: Sell) -> Instruction;
    fn get_create_idempotent_ata_ix(&self) -> Vec<Instruction>;
    fn get_create_ata_ix(&self) -> Instruction;
    fn get_close_wsol(&self) -> Instruction;
    fn get_wrap_sol(&self, sol_lamport: u64) -> Vec<Instruction>;
    fn global_volume_accumulator_pda() -> Pubkey;
    fn user_volume_accumulator_pda(user: &Pubkey) -> Pubkey;
    fn fee_config_pda() -> Pubkey;
    fn fee_program() -> Pubkey;
}

impl SellInstructionAccountsExt for SellInstructionAccounts {
    fn get_create_ata_ix(&self) -> Instruction {
        let create_ata_ix =
            spl_associated_token_account::instruction::create_associated_token_account(
                &self.user,
                &self.user,
                &self.base_mint,
                &self.base_token_program,
            );

        create_ata_ix
    }

    fn get_create_idempotent_ata_ix(&self) -> Vec<Instruction> {
        let create_ata_ix1 =
            spl_associated_token_account::instruction::create_associated_token_account_idempotent(
                &self.user,
                &self.user,
                &self.base_mint,
                &self.base_token_program,
            );

        let create_ata_ix2 =
            spl_associated_token_account::instruction::create_associated_token_account_idempotent(
                &self.user,
                &self.user,
                &self.quote_mint,
                &self.quote_token_program,
            );

        vec![create_ata_ix1, create_ata_ix2]
    }

    fn get_wrap_sol(&self, sol_lamport: u64) -> Vec<Instruction> {
        let wsol_ata = get_associated_token_address(&self.user, &WSOL);
        let transfer_ix = system_instruction::transfer(&self.user, &wsol_ata, sol_lamport);
        let wrap_ix = sync_native(&spl_token::ID, &wsol_ata).unwrap();

        vec![transfer_ix, wrap_ix]
    }

    fn get_close_wsol(&self) -> Instruction {
        let wsol_ata = get_associated_token_address(&self.user, &WSOL);

        let create_ata_ix = spl_token::instruction::close_account(
            &spl_token::ID,
            &wsol_ata,
            &self.user,
            &self.user,
            &[],
        )
        .unwrap();

        create_ata_ix
    }

    fn get_buy_ix(&self, buy_params: Buy) -> Instruction {
        let discriminator = [102, 6, 61, 18, 1, 218, 235, 234];
        let mut data = Vec::new();

        data.extend_from_slice(&discriminator);
        data.extend_from_slice(&buy_params.base_amount_out.to_le_bytes());
        data.extend_from_slice(&buy_params.max_quote_amount_in.to_le_bytes());

        let global_volume_accumulator = Self::global_volume_accumulator_pda();
        let user_volume_accumulator = Self::user_volume_accumulator_pda(&self.user);
        let fee_config = Self::fee_config_pda();
        let fee_program = Self::fee_program();

        // Then encode the struct fields using Borsh

        let accounts = vec![
            AccountMeta::new_readonly(self.pool, false), // #1 - Pool
            AccountMeta::new(self.user, true),           // #2 - User (Signer, Writable, Fee Payer)
            AccountMeta::new_readonly(self.global_config, false), // #3 - Global Config
            AccountMeta::new_readonly(self.base_mint, false), // #4 - Base Mint (WSOL)
            AccountMeta::new_readonly(self.quote_mint, false), // #5 - Quote Mint (TSFart)
            AccountMeta::new(self.user_base_token_account, false), // #6 - User Base Token Account
            AccountMeta::new(self.user_quote_token_account, false), // #7 - User Quote Token Account
            AccountMeta::new(self.pool_base_token_account, false), // #8 - Pool Base Token Account
            AccountMeta::new(self.pool_quote_token_account, false), // #9 - Pool Quote Token Account
            AccountMeta::new_readonly(self.protocol_fee_recipient, false), // #10 - Protocol Fee Recipient
            AccountMeta::new(self.protocol_fee_recipient_token_account, false), // #11 - Protocol Fee Recipient Token Account
            AccountMeta::new_readonly(self.base_token_program, false), // #12 - Base Token Program (Token Program)
            AccountMeta::new_readonly(self.quote_token_program, false), // #13 - Quote Token Program (Token Program)
            AccountMeta::new_readonly(self.system_program, false),      // #14 - System Program
            AccountMeta::new_readonly(self.associated_token_program, false), // #15 - Associated Token Program
            AccountMeta::new_readonly(self.event_authority, false), // #16 - Event Authority
            AccountMeta::new_readonly(self.program, false),         // #17 - Program (Pump.fun AMM)
            AccountMeta::new(self.coin_creator_vault_ata, false),   // #18 - Coin Creator Vault ATA
            AccountMeta::new_readonly(self.coin_creator_vault_authority, false), // #19 - Coin Creator Vault Authority
            AccountMeta::new(global_volume_accumulator, false),
            AccountMeta::new(user_volume_accumulator, false),
            AccountMeta::new(fee_config, false),
            AccountMeta::new(fee_program, false),
        ];

        Instruction {
            program_id: PUMPSWAP_PROGRAM_ID,
            accounts,
            data,
        }
    }

    fn get_sell_ix(&self, sell_params: Sell) -> Instruction {
        let discriminator = [51, 230, 133, 164, 1, 127, 131, 173];

        let mut data = Vec::new();

        data.extend_from_slice(&discriminator);
        data.extend_from_slice(&sell_params.base_amount_in.to_le_bytes());
        data.extend_from_slice(&sell_params.min_quote_amount_out.to_le_bytes());
        let fee_config = Self::fee_config_pda();
        let fee_program = Self::fee_program();

        let accounts = vec![
            AccountMeta::new_readonly(self.pool, false),                    // #1 - Pool
            AccountMeta::new(self.user, true), // #2 - User (Signer, Writable, Fee Payer)
            AccountMeta::new_readonly(self.global_config, false), // #3 - Global Config
            AccountMeta::new_readonly(self.base_mint, false), // #4 - Base Mint (WSOL)
            AccountMeta::new_readonly(self.quote_mint, false), // #5 - Quote Mint (TSFart)
            AccountMeta::new(self.user_base_token_account, false), // #6 - User Base Token Account
            AccountMeta::new(self.user_quote_token_account, false), // #7 - User Quote Token Account
            AccountMeta::new(self.pool_base_token_account, false), // #8 - Pool Base Token Account
            AccountMeta::new(self.pool_quote_token_account, false), // #9 - Pool Quote Token Account
            AccountMeta::new_readonly(self.protocol_fee_recipient, false), // #10 - Protocol Fee Recipient
            AccountMeta::new(self.protocol_fee_recipient_token_account, false), // #11 - Protocol Fee Recipient Token Account
            AccountMeta::new_readonly(self.base_token_program, false), // #12 - Base Token Program (Token Program)
            AccountMeta::new_readonly(self.quote_token_program, false), // #13 - Quote Token Program (Token Program)
            AccountMeta::new_readonly(self.system_program, false),      // #14 - System Program
            AccountMeta::new_readonly(self.associated_token_program, false), // #15 - Associated Token Program
            AccountMeta::new_readonly(self.event_authority, false), // #16 - Event Authority
            AccountMeta::new_readonly(self.program, false), // #17 - Program (Pump.fun AMM)
            AccountMeta::new(self.coin_creator_vault_ata, false), // #18 - Coin Creator Vault ATA
            AccountMeta::new_readonly(self.coin_creator_vault_authority, false), // #19 - Coin Creator Vault Authority
            AccountMeta::new_readonly(fee_config, false),
            AccountMeta::new_readonly(fee_program, false),
        ];

        Instruction {
            program_id: PUMPSWAP_PROGRAM_ID,
            accounts,
            data,
        }
    }

    // Pump program
    fn global_volume_accumulator_pda() -> Pubkey {
        let (global_volume_accumulator, _bump) = Pubkey::find_program_address(
            &[b"global_volume_accumulator"],
            &Pubkey::from_str_const("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA"),
        );
        global_volume_accumulator
    }

    fn user_volume_accumulator_pda(user: &Pubkey) -> Pubkey {
        let (user_volume_accumulator, _bump) = Pubkey::find_program_address(
            &[b"user_volume_accumulator", user.as_ref()],
            &Pubkey::from_str_const("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA"),
        );
        user_volume_accumulator
    }

    fn fee_config_pda() -> Pubkey {
        Pubkey::from_str_const("5PHirr8joyTMp9JMm6nW7hNDVyEYdkzDqazxPD7RaTjx")
    }

    fn fee_program() -> Pubkey {
        Pubkey::from_str_const("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ")
    }
}
