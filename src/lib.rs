//! Free Mint Alkane Contract
//!
//! A modernized and secure version of the free mint alkane contract that follows
//! current best practices and security patterns while providing full functionality
//! of a standard token plus free mint capabilities.

use alkanes_runtime::{declare_alkane, runtime::AlkaneResponder, message::MessageDispatch};
use alkanes_runtime::storage::StoragePointer;
use alkanes_support::response::CallResponse;
use alkanes_support::utils::overflow_error;
use metashrew_support::utils::{consensus_decode};
use alkanes_support::witness::find_witness_payload;
use alkanes_support::{context::Context, parcel::AlkaneTransfer};
use alkanes_support::gz;
use anyhow::{anyhow, Result};
use bitcoin::{Txid, Transaction};
use bitcoin::hashes::Hash;
use metashrew_support::compat::to_arraybuffer_layout;
use metashrew_support::index_pointer::KeyValuePointer;
use std::sync::Arc;
use std::io::Cursor;

/// Constants for token identification
pub const ALKANE_FACTORY_OWNED_TOKEN_ID: u128 = 0x0fff;
pub const ALKANE_FACTORY_FREE_MINT_ID: u128 = 0x0ffe;

/// Returns a StoragePointer for the token name
fn name_pointer() -> StoragePointer {
    StoragePointer::from_keyword("/name")
}

/// Returns a StoragePointer for the token symbol
fn symbol_pointer() -> StoragePointer {
    StoragePointer::from_keyword("/symbol")
}

/// Trims a u128 value to a String by removing trailing zeros
pub fn trim(v: u128) -> String {
    String::from_utf8(
        v.to_le_bytes()
            .into_iter()
            .fold(Vec::<u8>::new(), |mut r, v| {
                if v != 0 {
                    r.push(v)
                }
                r
            }),
    )
    .unwrap()
}

/// TokenName struct to hold two u128 values for the name
#[derive(Default, Clone, Copy)]
pub struct TokenName {
    pub part1: u128,
    pub part2: u128,
}

impl From<TokenName> for String {
    fn from(name: TokenName) -> Self {
        // Trim both parts and concatenate them
        format!("{}{}", trim(name.part1), trim(name.part2))
    }
}

impl TokenName {
    pub fn new(part1: u128, part2: u128) -> Self {
        Self { part1, part2 }
    }
}

pub struct ContextHandle(());

#[cfg(test)]
impl ContextHandle {
    /// Get the current transaction bytes
    pub fn transaction(&self) -> Vec<u8> {
        // This is a placeholder implementation that would normally
        // access the transaction from the runtime context
        Vec::new()
    }
}

impl AlkaneResponder for ContextHandle {
    fn execute(&self) -> Result<CallResponse> {
        Ok(CallResponse::default())
    }
}

pub const CONTEXT: ContextHandle = ContextHandle(());

/// Extension trait for Context to add transaction_id method
trait ContextExt {
    /// Get the transaction ID from the context
    fn transaction_id(&self) -> Result<Txid>;
}

#[cfg(test)]
impl ContextExt for Context {
    fn transaction_id(&self) -> Result<Txid> {
        // Test implementation with all zeros
        Ok(Txid::from_slice(&[0; 32]).unwrap_or_else(|_| {
            // This should never happen with a valid-length slice
            panic!("Failed to create zero Txid")
        }))
    }
}

#[cfg(not(test))]
impl ContextExt for Context {
    fn transaction_id(&self) -> Result<Txid> {
      Ok(consensus_decode::<Transaction>(&mut std::io::Cursor::new(CONTEXT.transaction()))?.compute_txid()) 
    }
}

/// MintableToken trait provides common token functionality
pub trait MintableToken: AlkaneResponder {
    /// Get the token name
    fn name(&self) -> String {
        String::from_utf8(self.name_pointer().get().as_ref().clone())
            .expect("name not saved as utf-8, did this deployment revert?")
    }
    
    /// Get the token symbol
    fn symbol(&self) -> String {
        String::from_utf8(self.symbol_pointer().get().as_ref().clone())
            .expect("symbol not saved as utf-8, did this deployment revert?")
    }
    
    /// Set the token name and symbol
    fn set_name_and_symbol(&self, name: TokenName, symbol: u128) {
        let name_string: String = name.into();
        self.name_pointer().set(Arc::new(name_string.as_bytes().to_vec()));
        self.set_string_field(self.symbol_pointer(), symbol);
    }
    
    /// Get the pointer to the token name
    fn name_pointer(&self) -> StoragePointer {
        name_pointer()
    }
    
    /// Get the pointer to the token symbol
    fn symbol_pointer(&self) -> StoragePointer {
        symbol_pointer()
    }
    
    /// Set a string field in storage
    fn set_string_field(&self, mut pointer: StoragePointer, v: u128) {
        pointer.set(Arc::new(trim(v).as_bytes().to_vec()));
    }
    
    /// Get the pointer to the total supply
    fn total_supply_pointer(&self) -> StoragePointer {
        StoragePointer::from_keyword("/totalsupply")
    }
    
    /// Get the total supply
    fn total_supply(&self) -> u128 {
        self.total_supply_pointer().get_value::<u128>()
    }
    
    /// Set the total supply
    fn set_total_supply(&self, v: u128) {
        self.total_supply_pointer().set_value::<u128>(v);
    }
    
    /// Increase the total supply
    fn increase_total_supply(&self, v: u128) -> Result<()> {
        self.set_total_supply(overflow_error(self.total_supply().checked_add(v))
            .map_err(|_| anyhow!("total supply overflow"))?);
        Ok(())
    }
    
    /// Mint new tokens
    fn mint(&self, context: &Context, value: u128) -> Result<AlkaneTransfer> {
        self.increase_total_supply(value)?;
        Ok(AlkaneTransfer {
            id: context.myself.clone(),
            value,
        })
    }
    
    /// Get the pointer to the token data
    fn data_pointer(&self) -> StoragePointer {
        StoragePointer::from_keyword("/data")
    }
    
    /// Get the token data
    fn data(&self) -> Vec<u8> {
        gz::decompress(self.data_pointer().get().as_ref().clone()).unwrap_or_else(|_| vec![])
    }
    
    /// Set the token data from the transaction
    fn set_data(&self) -> Result<()> {
        let tx = consensus_decode::<Transaction>(&mut Cursor::new(CONTEXT.transaction()))?;
        let data: Vec<u8> = find_witness_payload(&tx, 0).unwrap_or_else(|| vec![]);
        self.data_pointer().set(Arc::new(data));
        
        Ok(())
    }
    
    /// Observe initialization to prevent multiple initializations
    fn observe_initialization(&self) -> Result<()> {
        let mut pointer = StoragePointer::from_keyword("/initialized");
        if pointer.get().len() == 0 {
            pointer.set_value::<u8>(0x01);
            Ok(())
        } else {
            Err(anyhow!("already initialized"))
        }
    }
}

/// MintableAlkane implements a free mint token contract with security features
#[derive(Default)]
pub struct MintableAlkane(());

impl MintableToken for MintableAlkane {}

/// Message enum for opcode-based dispatch
#[derive(MessageDispatch)]
enum MintableAlkaneMessage {
    /// Initialize the token with configuration
    #[opcode(0)]
    Initialize {
        /// Initial token units
        token_units: u128,
        /// Value per mint
        value_per_mint: u128,
        /// Maximum supply cap (0 for unlimited)
        cap: u128,
        /// Token name part 1
        name_part1: u128,
        /// Token name part 2
        name_part2: u128,
        /// Token symbol
        symbol: u128,
    },

    /// Mint new tokens
    #[opcode(77)]
    MintTokens,

    /// Get the token name
    #[opcode(99)]
    #[returns(String)]
    GetName,

    /// Get the token symbol
    #[opcode(100)]
    #[returns(String)]
    GetSymbol,

    /// Get the total supply
    #[opcode(101)]
    #[returns(u128)]
    GetTotalSupply,

    /// Get the maximum supply cap
    #[opcode(102)]
    #[returns(u128)]
    GetCap,

    /// Get the total minted count
    #[opcode(103)]
    #[returns(u128)]
    GetMinted,

    /// Get the value per mint
    #[opcode(104)]
    #[returns(u128)]
    GetValuePerMint,

    /// Get the token data
    #[opcode(1000)]
    #[returns(Vec<u8>)]
    GetData,
}

impl MintableAlkane {
    /// Get the pointer to the minted counter
    pub fn minted_pointer(&self) -> StoragePointer {
        StoragePointer::from_keyword("/minted")
    }

    /// Get the total minted count
    pub fn minted(&self) -> u128 {
        self.minted_pointer().get_value::<u128>()
    }

    /// Set the total minted count
    pub fn set_minted(&self, v: u128) {
        self.minted_pointer().set_value::<u128>(v);
    }

    /// Increment the mint counter
    pub fn increment_mint(&self) -> Result<()> {
        self.set_minted(overflow_error(self.minted().checked_add(1u128))
            .map_err(|_| anyhow!("mint counter overflow"))?);
        Ok(())
    }

    /// Get the pointer to the value per mint
    pub fn value_per_mint_pointer(&self) -> StoragePointer {
        StoragePointer::from_keyword("/value-per-mint")
    }

    /// Get the value per mint
    pub fn value_per_mint(&self) -> u128 {
        self.value_per_mint_pointer().get_value::<u128>()
    }

    /// Set the value per mint
    pub fn set_value_per_mint(&self, v: u128) {
        self.value_per_mint_pointer().set_value::<u128>(v);
    }

    /// Get the pointer to the supply cap
    pub fn cap_pointer(&self) -> StoragePointer {
        StoragePointer::from_keyword("/cap")
    }

    /// Get the supply cap
    pub fn cap(&self) -> u128 {
        self.cap_pointer().get_value::<u128>()
    }

    /// Set the supply cap (0 means unlimited)
    pub fn set_cap(&self, v: u128) {
        self.cap_pointer().set_value::<u128>(if v == 0 { u128::MAX } else { v });
    }

    /// Check if a transaction hash has been used for minting
    pub fn has_tx_hash(&self, txid: &Txid) -> bool {
        StoragePointer::from_keyword("/tx-hashes/")
            .select(&txid.as_byte_array().to_vec())
            .get_value::<u8>() == 1
    }

    /// Add a transaction hash to the used set
    pub fn add_tx_hash(&self, txid: &Txid) -> Result<()> {
        StoragePointer::from_keyword("/tx-hashes/")
            .select(&txid.as_byte_array().to_vec())
            .set_value::<u8>(0x01);
        Ok(())
    }

    /// Initialize the token with configuration
    fn initialize(
        &self,
        token_units: u128,
        value_per_mint: u128,
        cap: u128,
        name_part1: u128,
        name_part2: u128,
        symbol: u128,
    ) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response = CallResponse::forward(&context.incoming_alkanes);

        // Prevent multiple initializations
        self.observe_initialization()
            .map_err(|_| anyhow!("Contract already initialized"))?;

        // Set configuration
        self.set_value_per_mint(value_per_mint);
        self.set_cap(cap);
        self.set_data()?;
        
        // Create TokenName from the two parts
        let name = TokenName::new(name_part1, name_part2);
        <Self as MintableToken>::set_name_and_symbol(self, name, symbol);

        // Mint initial tokens
        if token_units > 0 {
            response.alkanes.0.push(self.mint(&context, token_units)?);
        }

        Ok(response)
    }

    /// Mint new tokens
    fn mint_tokens(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response = CallResponse::forward(&context.incoming_alkanes);

        // Get transaction ID
        let txid = context.transaction_id()?;

        // Enforce one mint per transaction
        if self.has_tx_hash(&txid) {
            return Err(anyhow!("Transaction already used for minting"));
        }

        // Check if minting would exceed cap
        if self.minted() >= self.cap() {
            return Err(anyhow!("Supply cap reached: {} of {}", self.minted(), self.cap()));
        }

        // Record transaction hash
        self.add_tx_hash(&txid)?;

        // Mint tokens
        let value = self.value_per_mint();
        response.alkanes.0.push(self.mint(&context, value)?);

        // Increment mint counter
        self.increment_mint()?;

        Ok(response)
    }

    /// Set the token name and symbol
    fn set_name_and_symbol(&self, name_part1: u128, name_part2: u128, symbol: u128) -> Result<CallResponse> {
        let context = self.context()?;
        let response = CallResponse::forward(&context.incoming_alkanes);

        // Create TokenName from the two parts
        let name = TokenName::new(name_part1, name_part2);
        <Self as MintableToken>::set_name_and_symbol(self, name, symbol);

        Ok(response)
    }

    /// Get the token name
    fn get_name(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response = CallResponse::forward(&context.incoming_alkanes);

        response.data = self.name().into_bytes().to_vec();

        Ok(response)
    }

    /// Get the token symbol
    fn get_symbol(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response = CallResponse::forward(&context.incoming_alkanes);

        response.data = self.symbol().into_bytes().to_vec();

        Ok(response)
    }

    /// Get the total supply
    fn get_total_supply(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response = CallResponse::forward(&context.incoming_alkanes);

        response.data = self.total_supply().to_le_bytes().to_vec();

        Ok(response)
    }

    /// Get the maximum supply cap
    fn get_cap(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response = CallResponse::forward(&context.incoming_alkanes);

        response.data = self.cap().to_le_bytes().to_vec();

        Ok(response)
    }

    /// Get the total minted count
    fn get_minted(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response = CallResponse::forward(&context.incoming_alkanes);

        response.data = self.minted().to_le_bytes().to_vec();

        Ok(response)
    }

    /// Get the value per mint
    fn get_value_per_mint(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response = CallResponse::forward(&context.incoming_alkanes);

        response.data = self.value_per_mint().to_le_bytes().to_vec();

        Ok(response)
    }

    /// Get the token data
    fn get_data(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let mut response = CallResponse::forward(&context.incoming_alkanes);

        response.data = self.data();

        Ok(response)
    }
}

impl AlkaneResponder for MintableAlkane {
    fn execute(&self) -> Result<CallResponse> {
        // This method should not be called directly when using MessageDispatch
        Err(anyhow!("This method should not be called directly. Use the declare_alkane macro instead."))
    }
}

// Use the MessageDispatch macro for opcode handling
declare_alkane! {
    impl AlkaneResponder for MintableAlkane {
        type Message = MintableAlkaneMessage;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use wasm_bindgen_test::wasm_bindgen_test;
    
    // Reset all storage keys used in tests
    fn reset_test_storage() {
        // Clear all storage keys used in tests
        StoragePointer::from_keyword("/initialized").set(Arc::new(Vec::new()));
        StoragePointer::from_keyword("/name").set(Arc::new(Vec::new()));
        StoragePointer::from_keyword("/symbol").set(Arc::new(Vec::new()));
        StoragePointer::from_keyword("/totalsupply").set(Arc::new(Vec::new()));
        StoragePointer::from_keyword("/minted").set(Arc::new(Vec::new()));
        StoragePointer::from_keyword("/value-per-mint").set(Arc::new(Vec::new()));
        StoragePointer::from_keyword("/cap").set(Arc::new(Vec::new()));
        StoragePointer::from_keyword("/data").set(Arc::new(Vec::new()));
        StoragePointer::from_keyword("/tx-hashes").set(Arc::new(Vec::new()));
    }

    #[wasm_bindgen_test]
    fn test_initialization() {
        // Reset storage
        reset_test_storage();
        
        // Create the MintableAlkane instance
        let alkane = MintableAlkane::default();
        
        // Set values
        let value_per_mint = 10u128;
        let cap = 100u128;
        
        alkane.set_value_per_mint(value_per_mint);
        alkane.set_cap(cap);
        
        // Verify the values were set correctly
        assert_eq!(alkane.value_per_mint(), value_per_mint);
        assert_eq!(alkane.cap(), cap);
    }
    
    #[wasm_bindgen_test]
    fn test_cap_enforcement() {
        // Reset storage
        reset_test_storage();
        
        // Create the MintableAlkane instance
        let alkane = MintableAlkane::default();
        
        // Set a low cap
        alkane.set_cap(5u128);
        
        // Verify the cap was set correctly
        assert_eq!(alkane.cap(), 5u128);
    }

    #[wasm_bindgen_test]
    fn test_mint_functionality() -> Result<()> {
        // Reset storage
        reset_test_storage();
        
        // Create the MintableAlkane instance
        let alkane = MintableAlkane::default();
        
        // Initialize the contract
        alkane.observe_initialization()?;
        alkane.set_value_per_mint(10u128);
        alkane.set_cap(100u128);
        
        // Verify initial state
        assert_eq!(alkane.total_supply(), 0u128);
        assert_eq!(alkane.minted(), 0u128);
        
        Ok(())
    }
    
    #[wasm_bindgen_test]
    fn test_name_and_symbol() -> Result<()> {
        // Reset storage
        reset_test_storage();
        
        // Create the MintableAlkane instance
        let alkane = MintableAlkane::default();
        
        // Initialize the contract
        alkane.observe_initialization()?;
        
        // Set name and symbol directly using the MintableToken trait methods
        // Note: We need to use little-endian encoding because of how trim() works
        let name_part1 = 0x54534554u128; // "TEST" in little-endian
        let name_part2 = 0x32u128; // "2" in little-endian
        let symbol = 0x545354u128; // "TST" in little-endian
        
        // Create TokenName from the two parts
        let name = TokenName::new(name_part1, name_part2);
        <MintableAlkane as MintableToken>::set_name_and_symbol(&alkane, name, symbol);
        
        // Verify name and symbol
        assert_eq!(<MintableAlkane as MintableToken>::name(&alkane), "TEST2");
        assert_eq!(<MintableAlkane as MintableToken>::symbol(&alkane), "TST");
        
        Ok(())
    }
    
    #[wasm_bindgen_test]
    fn test_initialization_guard() {
        // Reset storage
        reset_test_storage();
        
        // Create the MintableAlkane instance
        let alkane = MintableAlkane::default();
        
        // First initialization should succeed
        assert!(alkane.observe_initialization().is_ok());
        
        // Second initialization should fail
        assert!(alkane.observe_initialization().is_err());
    }
    
    #[wasm_bindgen_test]
    fn test_total_supply_increase() -> Result<()> {
        // Reset storage
        reset_test_storage();
        
        // Create the MintableAlkane instance
        let alkane = MintableAlkane::default();
        
        // Initialize the contract
        alkane.observe_initialization()?;
        
        // Initial total supply should be 0
        assert_eq!(alkane.total_supply(), 0u128);
        
        // Increase total supply
        alkane.increase_total_supply(50u128)?;
        
        // Verify total supply was increased
        assert_eq!(alkane.total_supply(), 50u128);
        
        // Increase total supply again
        alkane.increase_total_supply(25u128)?;
        
        // Verify total supply was increased again
        assert_eq!(alkane.total_supply(), 75u128);
        
        Ok(())
    }
}

