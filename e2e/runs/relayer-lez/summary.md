# relayer-lez: root imports with the LEZ side failing (R.06)

- PASS A: outage: the round's LEZ calls fail ({"event":"root-import-failed","reason":"the devnet's reply has no body"}), nothing imported
- PASS A: the endpoint back: ImportRoot n = 2 applied once ({"leafCount":2,"applied":true})
- PASS B: the import's answer lost: kept in flight ({"leafCount":3,"keptInFlight":true,"reason":"the devnet's reply has no body"}), applied on the zone
- PASS B: next round: booked from the record ({"leafCount":3,"recovered":true}), nothing sent again
- PASS C: SIGKILLed with its ImportRoot at the zone and the answer held
- PASS C: restarted: booked from its record ({"leafCount":4,"recovered":true}), nothing sent again, a clean stop ({"reason":"SIGTERM"})
- PASS D: SIGKILLed with its ImportRoot built and held unsent
- PASS D: nothing reached the zone ([2,3,4])
- PASS D: restarted: its record waited out (1 pending, 1 expired), then n = 5 imported once ({"leafCount":5,"applied":true,"recovered":null})

9 passed, 0 failed
