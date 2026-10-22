#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::{StellarAssetClient, TokenClient},
    Address, BytesN, Env,
};

struct Fix<'a> {
    env: Env,
    bounty: RefractBountyEscrowClient<'a>,
    governance: Address,
    usdc_admin: StellarAssetClient<'a>,
}

fn setup<'a>() -> Fix<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let governance = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(governance.clone());
    let bounty_id = env.register_contract(None, RefractBountyEscrow);
    let bounty = RefractBountyEscrowClient::new(&env, &bounty_id);
    bounty.initialize(&governance, &sac.address());
    let usdc_admin = StellarAssetClient::new(&env, &sac.address());
    usdc_admin.mint(&governance, &1_000);
    bounty.fund(&governance, &1_000);
    Fix {
        env,
        bounty,
        governance,
        usdc_admin,
    }
}

fn hash(env: &Env, n: u8) -> BytesN<32> {
    BytesN::from_array(env, &[n; 32])
}

#[test]
fn relayed_claim_pays_the_claimant_who_signed() {
    let f = setup();
    let claimant = Address::generate(&f.env);
    let report = hash(&f.env, 1);
    f.bounty
        .open_award(&f.governance, &claimant, &250, &report);
    f.bounty.set_sponsor(&f.governance, &f.governance, &1_000, &100);
    f.bounty.claim(&claimant, &claimant, &250, &report);
    let token = TokenClient::new(&f.env, &f.usdc_admin.address);
    assert_eq!(token.balance(&claimant), 250);
}

#[test]
fn relayed_claim_rejects_a_tampered_destination() {
    let f = setup();
    let claimant = Address::generate(&f.env);
    let other = Address::generate(&f.env);
    let report = hash(&f.env, 2);
    f.bounty
        .open_award(&f.governance, &claimant, &100, &report);
    assert_eq!(
        f.bounty.try_claim(&claimant, &other, &100, &report),
        Err(Ok(BountyError::Unauthorized))
    );
}

#[test]
fn sponsor_revocation_fails_an_inflight_claim_and_keeps_the_credit() {
    let f = setup();
    let claimant = Address::generate(&f.env);
    let first = hash(&f.env, 3);
    let second = hash(&f.env, 4);
    f.bounty.set_sponsor(&f.governance, &f.governance, &1_000, &100);
    f.bounty
        .open_award(&f.governance, &claimant, &40, &first);
    f.bounty.claim(&claimant, &claimant, &40, &first);
    f.bounty.revoke_sponsorship(&f.governance);
    f.bounty
        .open_award(&f.governance, &claimant, &40, &second);
    assert_eq!(
        f.bounty.try_claim(&claimant, &claimant, &40, &second),
        Err(Ok(BountyError::SponsorshipRevoked))
    );
    let token = TokenClient::new(&f.env, &f.usdc_admin.address);
    assert_eq!(token.balance(&claimant), 40);
}

#[test]
fn sponsor_ceiling_rejects_the_claim() {
    let f = setup();
    let claimant = Address::generate(&f.env);
    let report = hash(&f.env, 5);
    f.bounty.set_sponsor(&f.governance, &f.governance, &10, &100);
    f.bounty
        .open_award(&f.governance, &claimant, &11, &report);
    assert_eq!(
        f.bounty.try_claim(&claimant, &claimant, &11, &report),
        Err(Ok(BountyError::SponsorCeilingReached))
    );
}

#[test]
fn unfunded_claimant_can_still_pull_an_opened_award() {
    let f = setup();
    f.env.ledger().with_mut(|li| {
        li.timestamp = 1_000;
    });
    let claimant = Address::generate(&f.env);
    let report = hash(&f.env, 6);
    f.bounty.set_sponsor(&f.governance, &f.governance, &500, &50);
    f.bounty
        .open_award(&f.governance, &claimant, &80, &report);
    let token = TokenClient::new(&f.env, &f.usdc_admin.address);
    assert_eq!(token.balance(&claimant), 0);
    f.bounty.claim(&claimant, &claimant, &80, &report);
    assert_eq!(token.balance(&claimant), 80);
}
