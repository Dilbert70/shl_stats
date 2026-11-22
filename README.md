# SHL Stats

Terminalprogram i Rust som hämtar matchresultat/schema och poängliga från [stats.swehockey.se](https://stats.swehockey.se/).

## Starta

```sh
cargo run
```

Programmet hittar SHL:s aktuella säsong via sajtens startsida och hämtar matcher, alla lagens spelare och tabell från säsongens egna sidor. Matchvyn visar de två senaste och två kommande omgångarna. `Tab` växlar mellan Matcher, Spelare och SHL-tabellen. Spelarvyn visar matcher, mål, assist, poäng och +/-. Sortera globalt med `g`/`a`/`p`/`x`; `G`/`A`/`P`/`X` sorterar först per lag. Tryck samma tangent igen för att vända sorteringsriktningen. Den sorterade kolumnen markeras i fetstil. Tryck `/` för att söka på spelarnamn, skriv söktext och tryck `Enter`; `Esc` rensar sökningen. SHL-tabellen visar lagens mål gjorda/insläppta, målskillnad och poäng. `↑`/`↓` eller `j`/`k` flyttar markeringen, `r` hämtar om data och `q` avslutar.

Första starten kräver nätverksåtkomst för att Cargo ska hämta beroenden. Sajten kan ändra sin HTML-struktur, vilket kan kräva att parsern uppdateras.
