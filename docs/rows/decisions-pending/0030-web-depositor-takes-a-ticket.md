# ADR 0030: A web depositor that takes a ticket from the user's Logos tool and never sees the seed

- **Date:** 21 November 2026
- **Status:** accepted; built (branch rfp-softs), not deployed
- **Requirements:** RFP-022 Soft.5, U.7

## Context

RFP-022 soft requirement 5 allows a supplementary approach for users whose funds sit in a common Ethereum wallet,
and asks that its security trade-offs be documented: browser code runs beside extensions, injected scripts and
phishing copies.

## Options considered

1. A web page that holds the bridge seed (or derives keys in the browser) and computes the commitment itself.
2. A static page that takes a ticket (`C`, `j`, vault, token, denomination) from a dry run of the user's own Logos
   tool and only relays the deposit through the injected wallet.
3. No web approach: users move funds to the Logos Ethereum module first.

## Decision

The second.

## Rationale and trade-offs

The seed, every LEZ key and the LEZ destination stay out of the browser, so a compromised page cannot claim, burn or
learn the destination. What it gives up: a tampered page or extension could swap `C` or the vault before the wallet
signs, so the user's comparison of both in the wallet's confirmation is the last check; the page asks for an exact
`approve`, has no network access of its own (CSP) and calls only allowlisted wallet methods.

## Sources

- branch rfp-softs, commit 57ba5a7e: web/depositor/index.html, web/depositor/README.md
