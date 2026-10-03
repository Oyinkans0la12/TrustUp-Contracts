#![cfg(test)]

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    Address, Env, IntoVal, String, Symbol, Val, Vec,
};

/// Helper function to set up the environment, contract, and test addresses.
fn setup<'a>(env: &'a Env) -> (MerchantRegistryContractClient<'a>, Address, Address) {
    // Using the updated register syntax
    let contract_id = env.register(MerchantRegistryContract, ());
    let client = MerchantRegistryContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let merchant = Address::generate(env);

    // Initialize the contract with the admin
    client.initialize(&admin);

    (client, admin, merchant)
}

#[test]
fn test_initialization() {
    let env = Env::default();
    // Using the updated register syntax
    let contract_id = env.register(MerchantRegistryContract, ());
    let client = MerchantRegistryContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    // Initial setup succeeds
    client.initialize(&admin);

    // Initializing twice throws an error
    let res = client.try_initialize(&admin);
    assert!(res.is_err());
}

#[test]
fn test_registration_flow() {
    let env = Env::default();
    let (client, admin, merchant) = setup(&env);

    env.mock_all_auths();
    // Advance ledger time to test registration_date
    env.ledger().with_mut(|l| l.timestamp = 1000000);

    let name = String::from_str(&env, "Galaxy Tech Supplies");
    client.register_merchant(&admin, &merchant, &name);

    assert!(client.is_active(&merchant));

    // get_merchant_info automatically unwraps on success in the test client
    let info = client.get_merchant_info(&merchant);
    assert_eq!(info.name, name);
    assert_eq!(info.registration_date, 1000000);
    assert_eq!(info.active, true);
    assert_eq!(info.total_sales, 0);
    assert_eq!(client.get_merchant_count(), 1);
}

#[test]
fn test_duplicate_prevention() {
    let env = Env::default();
    let (client, admin, merchant) = setup(&env);
    env.mock_all_auths();

    let name = String::from_str(&env, "Stellar Books");
    client.register_merchant(&admin, &merchant, &name);

    // Registering the exact same address again should fail
    let res = client.try_register_merchant(&admin, &merchant, &name);
    assert!(res.is_err());
}

#[test]
fn test_admin_only_access() {
    let env = Env::default();
    let (client, _admin, merchant) = setup(&env);
    env.mock_all_auths();

    // Create a rogue admin address
    let fake_admin = Address::generate(&env);
    let name = String::from_str(&env, "Rogue Merchant");

    // Use try_register_merchant to catch the expected Unauthorized error
    let res = client.try_register_merchant(&fake_admin, &merchant, &name);
    assert!(res.is_err());
}

#[test]
fn test_activation_and_deactivation() {
    let env = Env::default();
    let (client, admin, merchant) = setup(&env);
    env.mock_all_auths();

    let name = String::from_str(&env, "Nebula Cafe");
    client.register_merchant(&admin, &merchant, &name);

    assert!(client.is_active(&merchant));

    // Deactivate merchant
    client.deactivate_merchant(&admin, &merchant);
    assert!(!client.is_active(&merchant));
    assert_eq!(client.get_merchant_info(&merchant).active, false);

    // Activate merchant
    client.activate_merchant(&admin, &merchant);
    assert!(client.is_active(&merchant));
    assert_eq!(client.get_merchant_info(&merchant).active, true);
}

#[test]
fn test_set_merchant_status() {
    let env = Env::default();
    let (client, admin, merchant) = setup(&env);
    env.mock_all_auths();

    let name = String::from_str(&env, "Quasar Goods");
    client.register_merchant(&admin, &merchant, &name);

    // Merchant starts active
    assert!(client.is_active(&merchant));

    // Deactivate via set_merchant_status
    client.set_merchant_status(&admin, &merchant, &false);
    assert!(!client.is_active(&merchant));

    // Reactivate via set_merchant_status
    client.set_merchant_status(&admin, &merchant, &true);
    assert!(client.is_active(&merchant));

    // Non-admin must be rejected
    let fake_admin = Address::generate(&env);
    let res = client.try_set_merchant_status(&fake_admin, &merchant, &false);
    assert!(res.is_err());
}

#[test]
fn it_emits_merchant_registered_event_with_metadata() {
    let env = Env::default();
    let (client, admin, merchant) = setup(&env);
    env.mock_all_auths();

    let name = String::from_str(&env, "Orion Traders");
    client.register_merchant(&admin, &merchant, &name);

    let events: Vec<(Address, Vec<Val>, Val)> = env.events().all();
    let mut found_event = false;
    for event in events.iter() {
        let topics = event.1.clone();
        let event_type: Symbol = topics.get(0).unwrap().into_val(&env);

        if event_type == symbol_short!("MERCHTREG") {
            // The registered merchant is an indexed topic.
            let event_merchant: Address = topics.get(1).unwrap().into_val(&env);
            assert_eq!(event_merchant, merchant);

            // The payload carries the metadata stored for the merchant.
            let event_name: String = event.2.into_val(&env);
            assert_eq!(event_name, name);

            let info = client.get_merchant_info(&merchant);
            assert_eq!(info.name, event_name);
            assert_eq!(info.active, true);

            found_event = true;
            break;
        }
    }

    assert!(found_event, "MERCHTREG event not found");
}

#[test]
fn it_emits_merchant_status_event_on_toggle() {
    let env = Env::default();
    let (client, admin, merchant) = setup(&env);
    env.mock_all_auths();

    let name = String::from_str(&env, "Sirius Supplies");
    client.register_merchant(&admin, &merchant, &name);

    client.deactivate_merchant(&admin, &merchant);
    client.activate_merchant(&admin, &merchant);

    let expected_topic = Symbol::new(&env, "MERCHTSTATUS");
    let events: Vec<(Address, Vec<Val>, Val)> = env.events().all();

    let mut status_events: Vec<bool> = Vec::new(&env);
    for event in events.iter() {
        let topics = event.1.clone();
        let event_type: Symbol = topics.get(0).unwrap().into_val(&env);

        if event_type == expected_topic {
            let event_merchant: Address = topics.get(1).unwrap().into_val(&env);
            assert_eq!(event_merchant, merchant);
            status_events.push_back(event.2.into_val(&env));
        }
    }

    // Deactivation then activation must both be reported.
    assert_eq!(status_events.len(), 2);
    assert_eq!(status_events.get(0).unwrap(), false);
    assert_eq!(status_events.get(1).unwrap(), true);
}
