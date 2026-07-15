# Prove su database "usa e getta" con Docker

Ambienti pronti per provare Charon end-to-end **senza installare alcun client
di database** sull'host (usano il path puro-Rust). Servono solo Docker e la
toolchain Rust.

## PostgreSQL → PostgreSQL (eseguibile subito)

```bash
./postgres/run-postgres-test.sh          # avvia, testa, ripulisce
./postgres/run-postgres-test.sh --keep   # lascia il container attivo
```

Cosa fa, con **dry-run prima** e **esecuzione poi**:

1. `charon test` sulla sorgente;
2. **clone pieno** (DROP+CREATE+dati) `charon_src → charon_full`;
3. **clone data-only + mascheramento** email `charon_src → charon_dst`
   (preserva lo schema, TRUNCATE+dati) e **riallineo delle sequenze**;
4. verifica dei conteggi e che un nuovo `INSERT` senza id non violi la PK
   (prova che la sequenza è stata riallineata).

Il container espone Postgres su `localhost:55432` (utente `app` / `app_pw`).

## Oracle (script pronto; richiede Instant Client per le operazioni reali)

```bash
./oracle/run-oracle-test.sh
```

- Il **dry-run di `oracle-load`** gira **sempre** (legge il pacchetto SQL\*Loader
  di esempio in `oracle/sample-package/` e stampa i comandi `sqlldr`), anche
  senza Oracle e senza Instant Client.
- Le operazioni **reali** (test/clone puro-Rust, `sqlldr`) richiedono l'**Oracle
  Instant Client** sull'host (vedi `../PORTABILITY.md`). Lo script lo rileva e, se
  manca, salta con istruzioni.

Il container `gvenzl/oracle-free:slim` è grande e lento al primo avvio: attendi
`DATABASE IS READY TO USE!`. Service name `FREEPDB1`, schema di prova `JFORM_DEV`.

### Il pacchetto SQL\*Loader di esempio

`oracle/sample-package/` riproduce l'output "formato Loader" di SQL Developer:

- `CUSTOMERS.ctl` / `CUSTOMERS.ldr`, `ORDERS.ctl` / `ORDERS.ldr` — un control file
  e un file dati per tabella (formato della documentazione: `|` come separatore,
  `#…#` come delimitatore opzionale);
- `load_order.txt` — l'ordine di caricamento che rispetta i vincoli FK (rimpiazza
  in modo portabile gli script `script_load_*.sh`).
