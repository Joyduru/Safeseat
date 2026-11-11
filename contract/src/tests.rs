use super::*;
use near_sdk::test_utils::{get_logs, VMContextBuilder};
use near_sdk::testing_env;

const MS: u64 = 1_000_000; // ns per ms
const NOW_MS: u64 = 1_800_000_000_000;
const HOUR_MS: u64 = 3_600_000;

fn near(n: u128) -> NearToken {
    NearToken::from_near(n)
}
fn acc(s: &str) -> AccountId {
    s.parse().unwrap()
}

fn ctx(who: &str, deposit: NearToken, now_ms: u64) {
    testing_env!(VMContextBuilder::new()
        .predecessor_account_id(acc(who))
        .attached_deposit(deposit)
        .block_timestamp(now_ms * MS)
        .build());
}

/// Organizer creates: 10 NEAR tickets, cap 12 NEAR, 10% royalty, 3 seats,
/// starts in 2h, payout 60 minutes after the start.
fn setup() -> SafeSeat {
    ctx("org.testnet", NearToken::from_yoctonear(0), NOW_MS);
    let mut c = SafeSeat::default();
    let id = c.create_event(
        "Lagos Live".into(),
        "Eko Hall".into(),
        NOW_MS + 2 * HOUR_MS,
        60,
        near(10),
        near(12),
        1_000,
        3,
    );
    assert_eq!(id, 0);
    c
}

#[test]
fn buying_locks_money_in_the_ticket() {
    let mut c = setup();
    ctx("ada.testnet", near(11), NOW_MS);
    let id = c.buy_ticket(0);
    assert_eq!(id, "0-1");
    let t = c.get_ticket(id).unwrap();
    assert_eq!(t.owner_id, acc("ada.testnet"));
    assert_eq!(t.locked, near(10));
    assert_eq!(c.get_event(0).unwrap().sold, 1);
    assert!(get_logs()[0].contains("nft_mint"));
}

#[test]
#[should_panic(expected = "Attach the full ticket price")]
fn cannot_underpay() {
    let mut c = setup();
    ctx("ada.testnet", near(9), NOW_MS);
    c.buy_ticket(0);
}

#[test]
#[should_panic(expected = "Sold out")]
fn cannot_oversell() {
    let mut c = setup();
    for who in ["a.testnet", "b.testnet", "c.testnet", "d.testnet"] {
        ctx(who, near(10), NOW_MS);
        c.buy_ticket(0);
    }
}

#[test]
#[should_panic(expected = "above the resale cap")]
fn scalping_above_cap_is_blocked() {
    let mut c = setup();
    ctx("ada.testnet", near(10), NOW_MS);
    let id = c.buy_ticket(0);
    c.list_ticket(id, near(30));
}

#[test]
fn resale_moves_ticket_and_its_refund() {
    let mut c = setup();
    ctx("ada.testnet", near(10), NOW_MS);
    let id = c.buy_ticket(0);
    c.list_ticket(id.clone(), near(12));

    ctx("bola.testnet", near(12), NOW_MS);
    c.buy_resale(id.clone());

    let t = c.get_ticket(id.clone()).unwrap();
    assert_eq!(t.owner_id, acc("bola.testnet"));
    assert_eq!(t.locked, near(10), "refund stays with the ticket");
    assert!(t.listed_for.is_none());
    assert!(c.get_tickets_for_owner(acc("ada.testnet")).is_empty());
    assert_eq!(c.get_tickets_for_owner(acc("bola.testnet")).len(), 1);
    assert_eq!(c.nft_token(id).unwrap().owner_id, acc("bola.testnet"));
}

#[test]
#[should_panic(expected = "You don't own this ticket")]
fn only_owner_can_list() {
    let mut c = setup();
    ctx("ada.testnet", near(10), NOW_MS);
    let id = c.buy_ticket(0);
    ctx("mallory.testnet", NearToken::from_yoctonear(0), NOW_MS);
    c.list_ticket(id, near(11));
}

#[test]
fn cancel_refunds_current_holders() {
    let mut c = setup();
    ctx("ada.testnet", near(10), NOW_MS);
    c.buy_ticket(0);
    ctx("bola.testnet", near(10), NOW_MS);
    c.buy_ticket(0);

    ctx("org.testnet", NearToken::from_yoctonear(0), NOW_MS);
    let refunded = c.cancel_event(0);
    assert_eq!(refunded, near(20));
    assert_eq!(c.get_event(0).unwrap().status, EventStatus::Cancelled);
    for t in c.get_tickets_for_event(0) {
        assert!(t.locked.is_zero());
    }
}

#[test]
#[should_panic(expected = "Only the organizer can cancel")]
fn strangers_cannot_cancel() {
    let mut c = setup();
    ctx("mallory.testnet", NearToken::from_yoctonear(0), NOW_MS);
    c.cancel_event(0);
}

#[test]
#[should_panic(expected = "Payout unlocks after the safety window ends")]
fn organizer_cannot_collect_early() {
    let mut c = setup();
    ctx("ada.testnet", near(10), NOW_MS);
    c.buy_ticket(0);
    // Event has started but the 60 minute safety window hasn't passed.
    ctx("org.testnet", NearToken::from_yoctonear(0), NOW_MS + 2 * HOUR_MS + 10 * 60_000);
    c.claim_payout(0);
}

#[test]
fn organizer_collects_after_window() {
    let mut c = setup();
    ctx("ada.testnet", near(10), NOW_MS);
    c.buy_ticket(0);
    ctx("bola.testnet", near(10), NOW_MS);
    c.buy_ticket(0);
    ctx("org.testnet", NearToken::from_yoctonear(0), NOW_MS + 3 * HOUR_MS);
    assert_eq!(c.claim_payout(0), near(20));
    assert_eq!(c.get_event(0).unwrap().status, EventStatus::PaidOut);
}

#[test]
#[should_panic(expected = "This event is already closed")]
fn no_payout_after_cancel() {
    let mut c = setup();
    ctx("ada.testnet", near(10), NOW_MS);
    c.buy_ticket(0);
    ctx("org.testnet", NearToken::from_yoctonear(0), NOW_MS);
    c.cancel_event(0);
    ctx("org.testnet", NearToken::from_yoctonear(0), NOW_MS + 3 * HOUR_MS);
    c.claim_payout(0);
}

#[test]
fn gift_transfer_moves_refund() {
    let mut c = setup();
    ctx("ada.testnet", near(10), NOW_MS);
    let id = c.buy_ticket(0);
    ctx("ada.testnet", NearToken::from_yoctonear(1), NOW_MS);
    c.nft_transfer(acc("chi.testnet"), id.clone(), None, Some("gift".into()));
    let t = c.get_ticket(id).unwrap();
    assert_eq!(t.owner_id, acc("chi.testnet"));
    assert_eq!(t.locked, near(10));
    assert!(get_logs().iter().any(|l| l.contains("nft_transfer")));
}

#[test]
#[should_panic(expected = "Sales closed when the event started")]
fn no_sales_after_start() {
    let mut c = setup();
    ctx("ada.testnet", near(10), NOW_MS + 2 * HOUR_MS);
    c.buy_ticket(0);
}

#[test]
#[should_panic(expected = "Resale cap can't be lower than the ticket price")]
fn cap_must_cover_price() {
    ctx("org.testnet", NearToken::from_yoctonear(0), NOW_MS);
    let mut c = SafeSeat::default();
    c.create_event("X".into(), "Y".into(), NOW_MS + HOUR_MS, 60, near(10), near(5), 0, 5);
}
