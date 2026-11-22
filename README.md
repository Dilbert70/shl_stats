# SHL Stats

Terminalprogram i Rust som hämtar matchresultat/schema och poängliga från [stats.swehockey.se](https://stats.swehockey.se/).

## Starta

```sh
cargo run
```

Programmet hittar SHL:s aktuella säsong via sajtens startsida och hämtar matcher, alla lagens spelare och tabell från säsongens egna sidor. Matchvyn visar de två senaste och två kommande omgångarna. Spelarvyn låter namnkolumnen ta återstående terminalbredd. `Tab` växlar mellan Matcher, Spelare och SHL-tabellen. I spelarvyn sorterar `g` på mål, `a` på assist och `p` på poäng; tryck samma tangent igen för att byta sorteringsriktning. `↑`/`↓` eller `j`/`k` flyttar markeringen, `r` hämtar om data och `q` avslutar.

Första starten kräver nätverksåtkomst för att Cargo ska hämta beroenden. Sajten kan ändra sin HTML-struktur, vilket kan kräva att parsern uppdateras.
