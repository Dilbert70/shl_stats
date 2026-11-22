# SHL Stats

Terminalprogram i Rust som hämtar matchresultat/schema och poängliga från [stats.swehockey.se](https://stats.swehockey.se/).

## Starta

```sh
cargo run
```

Programmet hittar SHL:s aktuella säsong via sajtens startsida och hämtar matcher, alla lagens spelare och tabell från säsongens egna sidor. Matchvyn visar de två senaste och två kommande omgångarna. `Tab` växlar mellan Matcher, Spelare och SHL-tabellen. Spelarvyn visar matcher, mål, assist, poäng, +/-, mål-, assist- och poängsnitt per match. Sortera total statistik globalt med `g`/`a`/`p`/`x` och snitt med `m`/`b`/`e`; motsvarande versaler sorterar först per lag. I SHL-tabellen sorterar `p` på totalpoäng och `s` på poäng per match. Tryck samma tangent igen för att vända sorteringsriktningen. Den sorterade kolumnen markeras i fetstil. Tryck `/` för att söka på spelarnamn, skriv söktext och tryck `Enter`; `Esc` rensar sökningen. SHL-tabellen visar gjorda/insläppta mål, målskillnad, poäng och poäng per match. `↑`/`↓` eller `j`/`k` flyttar markeringen, `r` hämtar om data och `q` avslutar.

Första starten kräver nätverksåtkomst för att Cargo ska hämta beroenden. Sajten kan ändra sin HTML-struktur, vilket kan kräva att parsern uppdateras.
