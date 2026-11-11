//! SafeSeat: event tickets that hold their own refund.
//!
//! When a fan buys a ticket, the money does not go to the organizer.
//! It stays locked inside the ticket (an NFT) until one of three things happens:
//!
//! 1. The event happens: after the payout time, the organizer collects.
//! 2. The event is cancelled: every ticket refunds whoever holds it, instantly.
//! 3. The fan resells: price is capped, the organizer gets a royalty,
//!    and the locked refund moves with the ticket to the new owner.
//!
//! Tickets follow the NEP-171 view methods and events, so wallets and
//! explorers can display them.

use near_sdk::store::{LookupMap, Vector};
use near_sdk::{
    env, log, near, require, AccountId, BorshStorageKey, NearToken, Promise,
};

/// Keeps `cancel_event` and `claim_payout` within one transaction's gas.
pub const MAX_CAPACITY: u32 = 50;
/// Organizer royalty on resales can be at most 20%.
pub const MAX_ROYALTY_BPS: u16 = 2_000;
const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);

#[near(serializers = [borsh])]
#[derive(BorshStorageKey)]
enum StorageKey {
    Events,
    Tickets,
    Owners,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub enum EventStatus {
    /// Tickets can be bought and resold. Money is locked in tickets.
    OnSale,
    /// Organizer cancelled. Every ticket was refunded to its holder.
    Cancelled,
    /// Event happened and the organizer collected the locked money.
    PaidOut,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct Event {
    pub id: u64,
    pub organizer: AccountId,
    pub name: String,
    pub venue: String,
    /// Doors open (ms since epoch). Sales and resales stop here.
    pub starts_at_ms: u64,
    /// Earliest time the organizer can collect. Gives fans a safety window.
    pub payout_after_ms: u64,
    pub price: NearToken,
    pub max_resale: NearToken,
    pub royalty_bps: u16,
    pub capacity: u32,
    pub sold: u32,
    pub status: EventStatus,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct Ticket {
    pub token_id: String,
    pub event_id: u64,
    pub seat: u32,
    pub owner_id: AccountId,
    /// The refund this ticket is holding right now.
    pub locked: NearToken,
    pub listed_for: Option<NearToken>,
}

/// NEP-171 / NEP-177 shaped token for wallets and explorers.
#[near(serializers = [json])]
pub struct JsonToken {
    pub token_id: String,
    pub owner_id: AccountId,
    pub metadata: TokenMetadata,
}

#[near(serializers = [json])]
pub struct TokenMetadata {
    pub title: String,
    pub description: String,
    pub extra: String,
}

#[near(serializers = [json])]
pub struct ContractMetadata {
    pub spec: String,
    pub name: String,
    pub symbol: String,
}

#[near(contract_state)]
pub struct SafeSeat {
    events: Vector<Event>,
    tickets: LookupMap<String, Ticket>,
    owners: LookupMap<AccountId, Vec<String>>,
}

impl Default for SafeSeat {
    fn default() -> Self {
        Self {
            events: Vector::new(StorageKey::Events),
            tickets: LookupMap::new(StorageKey::Tickets),
            owners: LookupMap::new(StorageKey::Owners),
        }
    }
}

fn token_id(event_id: u64, seat: u32) -> String {
    format!("{event_id}-{seat}")
}

fn nep171_event(event: &str, data: String) {
    log!(
        "EVENT_JSON:{{\"standard\":\"nep171\",\"version\":\"1.0.0\",\"event\":\"{}\",\"data\":[{}]}}",
        event,
        data
    );
}

#[near]
impl SafeSeat {
    // ---------------------------------------------------------------
    // Organizer
    // ---------------------------------------------------------------

    /// Create an event. Returns its id.
    /// `payout_delay_minutes` is how long after the start the money stays
    /// locked (use 1440 for a day; small values are handy for demos).
    pub fn create_event(
        &mut self,
        name: String,
        venue: String,
        starts_at_ms: u64,
        payout_delay_minutes: u64,
        price: NearToken,
        max_resale: NearToken,
        royalty_bps: u16,
        capacity: u32,
    ) -> u64 {
        require!(!name.trim().is_empty(), "Give the event a name");
        require!(name.len() <= 80 && venue.len() <= 80, "Name and venue must be 80 characters or less");
        require!(starts_at_ms > env::block_timestamp_ms(), "Start time must be in the future");
        require!(!price.is_zero(), "Ticket price must be more than 0");
        require!(max_resale >= price, "Resale cap can't be lower than the ticket price");
        require!(royalty_bps <= MAX_ROYALTY_BPS, "Royalty can be at most 20%");
        require!(capacity >= 1 && capacity <= MAX_CAPACITY, "Capacity must be between 1 and 50");

        let id = self.events.len() as u64;
        self.events.push(Event {
            id,
            organizer: env::predecessor_account_id(),
            name,
            venue,
            starts_at_ms,
            payout_after_ms: starts_at_ms + payout_delay_minutes * 60_000,
            price,
            max_resale,
            royalty_bps,
            capacity,
            sold: 0,
            status: EventStatus::OnSale,
        });
        log!("Event {} created", id);
        id
    }

    /// Cancel the event. Every ticket refunds its current holder in this
    /// same transaction. No forms, no waiting.
    pub fn cancel_event(&mut self, event_id: u64) -> NearToken {
        let event = self.event_mut(event_id);
        require!(env::predecessor_account_id() == event.organizer, "Only the organizer can cancel");
        require!(event.status == EventStatus::OnSale, "This event is already closed");
        event.status = EventStatus::Cancelled;
        let sold = event.sold;

        let mut total: u128 = 0;
        for seat in 1..=sold {
            let ticket = self.tickets.get_mut(&token_id(event_id, seat)).expect("missing ticket");
            if !ticket.locked.is_zero() {
                total += ticket.locked.as_yoctonear();
                Promise::new(ticket.owner_id.clone()).transfer(ticket.locked).detach();
                ticket.locked = NearToken::from_yoctonear(0);
            }
            ticket.listed_for = None;
        }
        log!("Event {} cancelled. Refunded {} yoctoNEAR across {} tickets", event_id, total, sold);
        NearToken::from_yoctonear(total)
    }

    /// After the payout time, release the locked money to the organizer.
    /// Anyone can call it; the money only ever goes to the organizer.
    pub fn claim_payout(&mut self, event_id: u64) -> NearToken {
        let now = env::block_timestamp_ms();
        let event = self.event_mut(event_id);
        require!(event.status == EventStatus::OnSale, "This event is already closed");
        require!(now >= event.payout_after_ms, "Payout unlocks after the safety window ends");
        event.status = EventStatus::PaidOut;
        let organizer = event.organizer.clone();
        let sold = event.sold;

        let mut total: u128 = 0;
        for seat in 1..=sold {
            let ticket = self.tickets.get_mut(&token_id(event_id, seat)).expect("missing ticket");
            total += ticket.locked.as_yoctonear();
            ticket.locked = NearToken::from_yoctonear(0);
            ticket.listed_for = None;
        }
        if total > 0 {
            Promise::new(organizer).transfer(NearToken::from_yoctonear(total)).detach();
        }
        log!("Event {} paid out {} yoctoNEAR", event_id, total);
        NearToken::from_yoctonear(total)
    }

    // ---------------------------------------------------------------
    // Fans
    // ---------------------------------------------------------------

    /// Buy a ticket at face value. The money is locked in the ticket.
    #[payable]
    pub fn buy_ticket(&mut self, event_id: u64) -> String {
        let buyer = env::predecessor_account_id();
        let deposit = env::attached_deposit();
        let now = env::block_timestamp_ms();

        let event = self.event_mut(event_id);
        require!(event.status == EventStatus::OnSale, "This event is no longer selling tickets");
        require!(now < event.starts_at_ms, "Sales closed when the event started");
        require!(event.sold < event.capacity, "Sold out");
        require!(deposit >= event.price, "Attach the full ticket price");
        event.sold += 1;
        let seat = event.sold;
        let price = event.price;

        let id = token_id(event_id, seat);
        self.tickets.insert(
            id.clone(),
            Ticket {
                token_id: id.clone(),
                event_id,
                seat,
                owner_id: buyer.clone(),
                locked: price,
                listed_for: None,
            },
        );
        self.add_owner(&buyer, &id);
        nep171_event("nft_mint", format!("{{\"owner_id\":\"{buyer}\",\"token_ids\":[\"{id}\"]}}"));

        let change = deposit.saturating_sub(price);
        if !change.is_zero() {
            Promise::new(buyer).transfer(change).detach();
        }
        id
    }

    /// List your ticket for resale. Price can't go above the organizer's cap.
    pub fn list_ticket(&mut self, token_id: String, price: NearToken) {
        let ticket = self.ticket_for_owner(&token_id);
        let event = self.event_ref(ticket.event_id);
        require!(event.status == EventStatus::OnSale, "This event is closed");
        require!(env::block_timestamp_ms() < event.starts_at_ms, "Resales closed when the event started");
        require!(!price.is_zero(), "Price must be more than 0");
        require!(
            price <= event.max_resale,
            format!("Price is above the resale cap of {}", event.max_resale.exact_amount_display())
        );
        self.tickets.get_mut(&token_id).unwrap().listed_for = Some(price);
    }

    pub fn unlist_ticket(&mut self, token_id: String) {
        self.ticket_for_owner(&token_id);
        self.tickets.get_mut(&token_id).unwrap().listed_for = None;
    }

    /// Buy a listed ticket. Seller gets paid, organizer gets the royalty,
    /// and the locked refund moves to you with the ticket.
    #[payable]
    pub fn buy_resale(&mut self, token_id: String) {
        let buyer = env::predecessor_account_id();
        let deposit = env::attached_deposit();
        let ticket = self.tickets.get(&token_id).expect("Ticket not found").clone();
        let price = ticket.listed_for.expect("This ticket isn't for sale");
        let event = self.event_ref(ticket.event_id).clone();
        require!(event.status == EventStatus::OnSale, "This event is closed");
        require!(env::block_timestamp_ms() < event.starts_at_ms, "Resales closed when the event started");
        require!(buyer != ticket.owner_id, "You already own this ticket");
        require!(deposit >= price, "Attach the full resale price");

        let royalty = price.as_yoctonear() * event.royalty_bps as u128 / 10_000;
        let to_seller = price.as_yoctonear() - royalty;
        if royalty > 0 {
            Promise::new(event.organizer.clone()).transfer(NearToken::from_yoctonear(royalty)).detach();
        }
        Promise::new(ticket.owner_id.clone()).transfer(NearToken::from_yoctonear(to_seller)).detach();
        let change = deposit.saturating_sub(price);
        if !change.is_zero() {
            Promise::new(buyer.clone()).transfer(change).detach();
        }
        self.move_ticket(&token_id, &buyer, Some("resale"));
    }

    // ---------------------------------------------------------------
    // NEP-171 core
    // ---------------------------------------------------------------

    /// Gift a ticket. The locked refund goes with it.
    /// Requires exactly 1 yoctoNEAR, like every NEP-171 transfer.
    #[payable]
    pub fn nft_transfer(
        &mut self,
        receiver_id: AccountId,
        token_id: String,
        approval_id: Option<u32>,
        memo: Option<String>,
    ) {
        let _ = approval_id;
        require!(env::attached_deposit() == ONE_YOCTO, "Requires attached deposit of exactly 1 yoctoNEAR");
        let ticket = self.ticket_for_owner(&token_id);
        require!(receiver_id != ticket.owner_id, "You already own this ticket");
        self.move_ticket(&token_id, &receiver_id, memo.as_deref());
    }

    pub fn nft_token(&self, token_id: String) -> Option<JsonToken> {
        self.tickets.get(&token_id).map(|t| self.to_json_token(t))
    }

    pub fn nft_tokens_for_owner(
        &self,
        account_id: AccountId,
        from_index: Option<String>,
        limit: Option<u32>,
    ) -> Vec<JsonToken> {
        let start: usize = from_index.and_then(|s| s.parse().ok()).unwrap_or(0);
        self.owners
            .get(&account_id)
            .map(|ids| {
                ids.iter()
                    .skip(start)
                    .take(limit.unwrap_or(50) as usize)
                    .filter_map(|id| self.tickets.get(id))
                    .map(|t| self.to_json_token(t))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn nft_supply_for_owner(&self, account_id: AccountId) -> String {
        self.owners.get(&account_id).map(|v| v.len()).unwrap_or(0).to_string()
    }

    pub fn nft_metadata(&self) -> ContractMetadata {
        ContractMetadata {
            spec: "nft-1.0.0".into(),
            name: "SafeSeat Tickets".into(),
            symbol: "SEAT".into(),
        }
    }

    // ---------------------------------------------------------------
    // Views for the app
    // ---------------------------------------------------------------

    pub fn get_event(&self, event_id: u64) -> Option<Event> {
        self.events.get(event_id as u32).cloned()
    }

    pub fn get_events(&self, from_index: Option<u64>, limit: Option<u64>) -> Vec<Event> {
        self.events
            .iter()
            .skip(from_index.unwrap_or(0) as usize)
            .take(limit.unwrap_or(50) as usize)
            .cloned()
            .collect()
    }

    pub fn get_ticket(&self, token_id: String) -> Option<Ticket> {
        self.tickets.get(&token_id).cloned()
    }

    pub fn get_tickets_for_event(&self, event_id: u64) -> Vec<Ticket> {
        let sold = self.events.get(event_id as u32).map(|e| e.sold).unwrap_or(0);
        (1..=sold)
            .filter_map(|seat| self.tickets.get(&token_id(event_id, seat)).cloned())
            .collect()
    }

    pub fn get_tickets_for_owner(&self, account_id: AccountId) -> Vec<Ticket> {
        self.owners
            .get(&account_id)
            .map(|ids| ids.iter().filter_map(|id| self.tickets.get(id).cloned()).collect())
            .unwrap_or_default()
    }

    // ---------------------------------------------------------------
    // Internal
    // ---------------------------------------------------------------

    fn event_ref(&self, event_id: u64) -> &Event {
        self.events.get(event_id as u32).expect("Event not found")
    }

    fn event_mut(&mut self, event_id: u64) -> &mut Event {
        self.events.get_mut(event_id as u32).expect("Event not found")
    }

    fn ticket_for_owner(&self, token_id: &String) -> Ticket {
        let ticket = self.tickets.get(token_id).expect("Ticket not found");
        require!(ticket.owner_id == env::predecessor_account_id(), "You don't own this ticket");
        ticket.clone()
    }

    fn add_owner(&mut self, owner: &AccountId, id: &String) {
        let mut ids = self.owners.get(owner).cloned().unwrap_or_default();
        ids.push(id.clone());
        self.owners.insert(owner.clone(), ids);
    }

    fn remove_owner(&mut self, owner: &AccountId, id: &String) {
        if let Some(ids) = self.owners.get_mut(owner) {
            ids.retain(|x| x != id);
        }
    }

    fn move_ticket(&mut self, id: &String, to: &AccountId, memo: Option<&str>) {
        let from = self.tickets.get(id).unwrap().owner_id.clone();
        self.remove_owner(&from, id);
        self.add_owner(to, id);
        let t = self.tickets.get_mut(id).unwrap();
        t.owner_id = to.clone();
        t.listed_for = None;
        let memo_json = memo.map(|m| format!(",\"memo\":{:?}", m)).unwrap_or_default();
        nep171_event(
            "nft_transfer",
            format!("{{\"old_owner_id\":\"{from}\",\"new_owner_id\":\"{to}\",\"token_ids\":[\"{id}\"]{memo_json}}}"),
        );
    }

    fn to_json_token(&self, t: &Ticket) -> JsonToken {
        let e = self.event_ref(t.event_id);
        JsonToken {
            token_id: t.token_id.clone(),
            owner_id: t.owner_id.clone(),
            metadata: TokenMetadata {
                title: format!("{} - Seat {}", e.name, t.seat),
                description: format!(
                    "{} at {}. This ticket holds its own refund until the event happens.",
                    e.name, e.venue
                ),
                extra: format!(
                    "{{\"event_id\":{},\"locked_yocto\":\"{}\"}}",
                    t.event_id,
                    t.locked.as_yoctonear()
                ),
            },
        }
    }
}

#[cfg(test)]
mod tests;
