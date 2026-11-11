//! What each step makes public, and what the user's current choices would leak (RFP-022 U.05–U.07).
//!
//! The same table and the same rules as the mini-app (`modules/ferrata_bridge_ui/qml/js/privacy.js`),
//! so that the tool and the mini-app say the same thing for the same choice. Sources there: the
//! proposal's §B6 field table and `docs/privacy/RESULTS.md`; the residual correlations are those
//! `docs/trust.md` lists. Levels: 0 fine, 1 a correlation the user accepts, 2 a direct link.

/// What one step publishes on each chain, and what it keeps private.
pub struct Step {
    pub title: &'static str,
    pub public_eth: &'static [&'static str],
    pub public_lez: &'static [&'static str],
    pub hidden: &'static [&'static str],
}

pub const DEPOSIT: Step = Step {
    title: "Deposit (Ethereum)",
    public_eth: &[
        "your depositor address",
        "the token",
        "the denomination and its fees",
        "the time",
        "the commitment C (random-looking)",
    ],
    public_lez: &[],
    hidden: &["the LEZ account that will receive the tokens (C commits to it)", "which later claim spends it"],
};

pub const CLAIM: Step = Step {
    title: "Claim (LEZ)",
    public_eth: &[],
    public_lez: &[
        "the nullifier N (fresh randomness)",
        "the denomination",
        "which imported root was read",
        "a public destination and its balance, if you chose one",
    ],
    hidden: &["which deposit is claimed (no leaf index, no depositor)", "a private destination and its balance"],
};

/// The plaintext claim: everything of [`CLAIM`], and the whole witness.
pub const CLAIM_PLAINTEXT: Step = Step {
    title: "Plaintext claim (LEZ)",
    public_eth: &[],
    public_lez: &[
        "the whole witness: the leaf index, the depositor, j, the secret s and the destination",
        "the nullifier N",
        "the denomination",
        "which imported root was read",
        "the destination and its balance",
    ],
    hidden: &[],
};

pub const BURN: Step = Step {
    title: "Burn (LEZ)",
    public_eth: &[],
    public_lez: &[
        "the burn note: token, denomination, r, the recipient encrypted (c_out)",
        "the holder account, if it is public",
    ],
    hidden: &["the Ethereum recipient (encrypted to you)", "a private holder and its balance"],
};

pub const RELEASE: Step = Step {
    title: "Release (Ethereum)",
    public_eth: &[
        "the recipient address",
        "the token and amount",
        "NOut (fresh randomness)",
        "the burn root used",
        "the time",
        "the relayer that sent it (it pays the gas)",
    ],
    public_lez: &[],
    hidden: &["which burn is released (no burn index)", "the LEZ holder"],
};

/// One consequence of the current choices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leak {
    pub level: u8,
    pub text: String,
}

fn leak(level: u8, text: impl Into<String>) -> Leak {
    Leak { level, text: text.into() }
}

/// The choices of a deposit.
pub struct DepositChoice<'a> {
    pub private: bool,
    /// The claim's hold, in hours (`None`: no delay).
    pub delay_hours: Option<u64>,
    pub relayer: bool,
    pub account: u32,
    pub gap: u32,
    pub denomination: &'a str,
}

/// privacy.js `leaks`.
pub fn deposit_leaks(c: &DepositChoice) -> Vec<Leak> {
    let mut out = vec![leak(
        0,
        format!("Fixed denomination: your deposit looks like every other deposit of {}.", c.denomination),
    )];
    if c.private {
        out.push(leak(0, "Private destination: the receiving account and its balance stay private on LEZ."));
    } else {
        out.push(leak(
            2,
            format!(
                "Public destination: the account and its balance become public on LEZ when claimed; anyone can see it received {} at that time.",
                c.denomination
            ),
        ));
    }
    match c.delay_hours {
        Some(h) => out.push(leak(0, format!("Delayed claim: held for {h} h, so the claim's time does not point at your deposit."))),
        None => out.push(leak(
            1,
            "No delay: a claim minutes after a deposit narrows the deposits it could be to those made just before.",
        )),
    }
    if c.relayer {
        out.push(leak(0, "Permissionless relayer: a relayer submits and pays the LEZ fee; no fee payment ties the claim to you."));
    } else {
        out.push(leak(1, "Self-submitted: the account paying the LEZ fee is linked to the claim."));
    }
    if c.account >= c.gap {
        out.push(leak(
            2,
            format!(
                "Account /{} is outside the recovery window ({}): a restored wallet would not find it.",
                c.account, c.gap
            ),
        ));
    }
    out
}

/// The choices of a claim.
pub struct ClaimChoice {
    pub private_claim: bool,
    /// The destination's kind (`Some(true)`: private).
    pub dest_private: Option<bool>,
    /// The deposit is still within its hold.
    pub held: bool,
    pub skip_delay: bool,
    /// No public account pays the LEZ fee: a private claim pays it inside its proof from the private
    /// account (LEZ's private fee path), or a relayer pays it.
    pub fee_hidden: bool,
}

/// privacy.js `claimLeaks`; the fee line is `leaks`' (the mini-app shows it at the deposit), with the
/// private fee path a private claim takes.
pub fn claim_leaks(c: &ClaimChoice) -> Vec<Leak> {
    let mut out = Vec::new();
    if c.private_claim {
        out.push(leak(0, "Private claim: proven without saying which deposit it spends (no leaf index, no depositor)."));
    } else {
        out.push(leak(2, "Plaintext claim: names on LEZ which deposit paid which account."));
    }
    match c.dest_private {
        Some(false) => out.push(leak(2, "Public destination: the account and its new balance are public.")),
        Some(true) => out.push(leak(0, "Private destination: the account and its balance stay private.")),
        None => {}
    }
    if c.held && c.skip_delay {
        out.push(leak(1, "Claiming before your random delay ends: the claim's time narrows which deposits it could be."));
    } else if c.held {
        out.push(leak(0, "Held by your random delay: the claim will not follow the deposit closely in time."));
    }
    if c.fee_hidden {
        out.push(leak(0, "The LEZ fee is paid inside the proof (or by a relayer): no public fee payment ties the claim to you."));
    } else {
        out.push(leak(1, "Self-submitted: the account paying the LEZ fee is linked to the claim."));
    }
    out
}

/// privacy.js `burnLeaks`.
pub fn burn_leaks(private_holder: bool, fresh_recipient: bool, relayer: bool) -> Vec<Leak> {
    let mut out = Vec::new();
    if private_holder {
        out.push(leak(0, "Burn from a private account: the holder stays private."));
    } else {
        out.push(leak(2, "Burn from a public account: the holder, its balance and the burn are public on LEZ."));
    }
    out.extend(release_leaks(fresh_recipient, relayer));
    out
}

/// The release's part of privacy.js `burnLeaks` (the recipient and who sends).
pub fn release_leaks(fresh_recipient: bool, relayer: bool) -> Vec<Leak> {
    let mut out = Vec::new();
    if fresh_recipient {
        out.push(leak(0, "Fresh recipient address: nothing on Ethereum ties the release to your depositor address."));
    } else {
        out.push(leak(1, "The recipient is an address you used before: the release is linked to it."));
    }
    if relayer {
        out.push(leak(0, "Released through a relayer: the recipient needs no ETH for gas."));
    } else {
        out.push(leak(1, "You send the release yourself: your gas payer is linked to the recipient."));
    }
    out
}

/// privacy.js `exposure`.
pub fn exposure(list: &[Leak]) -> &'static str {
    match list.iter().map(|l| l.level).max().unwrap_or(0) {
        0 => "Minimal: only the residual correlations (token, denomination, time)",
        1 => "Elevated: a timing or fee-payer correlation",
        _ => "High: a direct link is public",
    }
}

fn join(items: &[&str]) -> String {
    if items.is_empty() {
        "nothing".to_string()
    } else {
        items.join("; ")
    }
}

/// The lines every command prints before it sends: what the step publishes on each chain, what it
/// keeps private, and the exposure of the current choices, worst first marked `!!`.
pub fn render(step: &Step, leaks: &[Leak]) -> Vec<String> {
    let mut out = vec![
        format!("what becomes public ({}):", step.title),
        format!("  public on Ethereum: {}", join(step.public_eth)),
        format!("  public on LEZ:      {}", join(step.public_lez)),
        format!("  stays private:      {}", join(step.hidden)),
        format!("privacy of these choices: {}", exposure(leaks)),
    ];
    for l in leaks {
        let mark = match l.level {
            0 => "  ok ",
            1 => "  !  ",
            _ => "  !! ",
        };
        out.push(format!("{mark}{}", l.text));
    }
    if leaks.iter().any(|l| l.level >= 2) {
        out.push(
            "WARNING: these choices make a direct link public on a chain, for everyone and for good. The private path is the default; drop the option that chose this path to use it."
                .to_string(),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_minimal_and_public_paths_warn() {
        let d = DepositChoice {
            private: true,
            delay_hours: Some(3),
            relayer: true,
            account: 0,
            gap: 8,
            denomination: "0.1 ETH",
        };
        assert_eq!(exposure(&deposit_leaks(&d)), exposure(&[]));
        let public = DepositChoice { private: false, ..d };
        let lines = render(&DEPOSIT, &deposit_leaks(&public));
        assert!(lines.iter().any(|l| l.starts_with("WARNING")));
        assert!(lines.iter().any(|l| l.contains("High: a direct link is public")));
        let claim = ClaimChoice { private_claim: true, dest_private: Some(true), held: true, skip_delay: false, fee_hidden: true };
        assert_eq!(claim_leaks(&claim).iter().map(|l| l.level).max(), Some(0));
        let skipped = ClaimChoice { skip_delay: true, ..claim };
        assert_eq!(exposure(&claim_leaks(&skipped)), "Elevated: a timing or fee-payer correlation");
        assert_eq!(burn_leaks(false, true, true)[0].level, 2);
    }
}
