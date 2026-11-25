# seqnet gasdrop: the network gas-drop relayer by default (U.5, Priv.7), on a running LEZ sequencer

- PASS the gateway publishes the network's relayer 0xbe4749d897560029caac6c340ab2d38409dc10755067b1fb97cdcf513840b551 (guest 0xce3ad533d2c72b38c9779f1c692dd784e3fd754612808696e1362da49c7f8125, 200000000 native LEZ per deposit, for claim and burn)
- PASS U.5/Priv.7: with no setting the claim's dry run names the network's relayer, read from the gateway, as fee payer: fee payer: a fresh one-time private account, credited (sealed) by the network's gas-drop relayer 0xbe4749d897560029caac6c340ab2d38409dc10755067b1fb97cdcf513840b…
- PASS U.5/Priv.7: the private claim applied, its fee paid by one-time account 0xa94c201bbb95f7e5770bf20cc4d09b48620066c5d32a6780d1d6b2564afc06f3 from the network relayer's sealed credit (job posted to the gateway: went to http://127.0.0.1:18084/relayer/gas-drop (job gas-drop-cd6efdc213344cb4703558dba09755f76699b31837e3c6f271bad16311b1adad.json); relayer: {"tx":"48c1cd515473c683328bfcd3f6d7afa46ed2bce63ff463e4bd0b6a66e6162de4","amount":"200000000","sealed":true,"sends":1})
- PASS the drop is recorded beside the wallet (ferrata-gas-drops.json: N -> the one-time account and the inbox URL)
- PASS nothing funded the user: seqnet-fund ran for the relayer only
- PASS U.5/Priv.7: with no setting the burn's dry run names the drop's one-time account as fee payer: fee payer: the one-time account 0xa94c201bbb95f7e5770bf20cc4d09b48620066c5d32a6780d1d6b2564afc06f3 of a gas drop, from the 189900781 native LEZ its relayer's credit left after your claim; nothing is a…
- PASS Priv.7: --no-gas-drop (the opt-out) names the user's own private account and what its funding reveals
- PASS U.5/Priv.7: the private burn applied, its fee paid by the drop's one-time account 0xa94c201bbb95f7e5770bf20cc4d09b48620066c5d32a6780d1d6b2564afc06f3 (the user holds no native LEZ elsewhere)

8 passed, 0 failed
