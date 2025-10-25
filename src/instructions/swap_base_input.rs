use carbon_raydium_cpmm_decoder::{
    PROGRAM_ID as CPMM_PROGRAM_ID,
    instructions::swap_base_input::{SwapBaseInput, SwapBaseInputInstructionAccounts},
};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

pub trait SwapBaseInputInstructionAccountsExt {
    fn get_swap_base_input_ix(&self, swap_base_input_param: SwapBaseInput) -> Instruction;
    fn get_create_idempotent_ata_ix(
        &self,
        input_mint: Pubkey,
        output_mint: Pubkey,
    ) -> Vec<Instruction>;
    fn get_create_ata_ix(&self) -> Instruction;
}

impl SwapBaseInputInstructionAccountsExt for SwapBaseInputInstructionAccounts {
    fn get_create_ata_ix(&self) -> Instruction {
        let create_ata_ix =
            spl_associated_token_account::instruction::create_associated_token_account(
                &self.payer,
                &self.payer,
                &self.input_token_mint,
                &self.input_token_program,
            );

        create_ata_ix
    }

    fn get_create_idempotent_ata_ix(
        &self,
        input_mint: Pubkey,
        output_mint: Pubkey,
    ) -> Vec<Instruction> {
        let create_ata_input_ix =
            spl_associated_token_account::instruction::create_associated_token_account_idempotent(
                &self.payer,
                &self.payer,
                &input_mint,
                &self.input_token_program,
            );

        let create_ata_output_ix =
            spl_associated_token_account::instruction::create_associated_token_account_idempotent(
                &self.payer,
                &self.payer,
                &output_mint,
                &self.output_token_program,
            );

        vec![create_ata_input_ix, create_ata_output_ix]
    }

    fn get_swap_base_input_ix(&self, swap_base_input_param: SwapBaseInput) -> Instruction {
        let discriminator = [146, 190, 90, 218, 196, 30, 51, 222];
        let mut data = Vec::new();

        data.extend_from_slice(&discriminator);
        data.extend_from_slice(&swap_base_input_param.amount_in.to_le_bytes());
        data.extend_from_slice(&swap_base_input_param.minimum_amount_out.to_le_bytes());

        let accounts = vec![
            AccountMeta::new(self.payer, true),
            AccountMeta::new(self.authority, false),
            AccountMeta::new(self.amm_config, false),
            AccountMeta::new(self.pool_state, false),
            AccountMeta::new(self.input_token_account, false),
            AccountMeta::new(self.output_token_account, false),
            AccountMeta::new(self.input_vault, false),
            AccountMeta::new(self.output_vault, false),
            AccountMeta::new(self.input_token_program, false),
            AccountMeta::new(self.output_token_program, false),
            AccountMeta::new(self.input_token_mint, false),
            AccountMeta::new(self.output_token_mint, false),
            AccountMeta::new(self.observation_state, false),
        ];

        Instruction {
            program_id: CPMM_PROGRAM_ID,
            accounts,
            data,
        }
    }
}
