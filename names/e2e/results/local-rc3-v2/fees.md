# Fees charged on the local rc3 sequencer (measured: the signer's balance before and after; native units)
| action | fee |
|---|---:|
| alice: commit alice (a hash; the name stays hidden) | 269432 |
| mallory: commit alice with now_ms 105 s back (the copier) | 269472 |
| mallory: reveal alice first (registered, provisional) | 570736 |
| alice: reveal alice in her window (older commitment: Displaced) | 593176 |
| mallory: forget (the names that left her) | 332960 |
| alice: set-primary alice | 334760 |
| alice: set-target alice, add a private receiver | 462960 |
| alice: renew alice | 360328 |
| alice: offer alice to bob (transfer) | 372928 |
| bob: accept alice | 434112 |
| alice: forget (the names that left her) | 338976 |
| bob: set-primary alice | 344336 |
| bob: release alice | 355088 |
