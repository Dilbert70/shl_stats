# SHL Stats

Ett terminalprogram i Rust som visar matchresultat, spelarstatistik, SHL-tabellen och målvaktsstatistik från [stats.swehockey.se](https://stats.swehockey.se/). Programmet hämtar aktuell säsong när det startar och behöver internetanslutning för att läsa statistiken.

## Krav

- Rust och Cargo, installerade via [rustup](https://rustup.rs/).
- En terminal som stöder tangentbordsinmatning och färger.
- Internetanslutning vid körning. Cargo hämtar också projektets beroenden vid första bygget.

Installera Rust och Cargo via [rustup](https://rustup.rs/) om de saknas. På Linux kan du köra den officiella installationsskriptet:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"
```

## Installera och bygg

Klona projektet och installera programmet:

```sh
git clone https://github.com/Dilbert70/shl_stats.git
cd shl_stats
cargo install --path .
```

Cargo installerar programmet i `~/.cargo/bin`. Starta det sedan med:

```sh
shl_stats
```

För att bygga en optimerad version lokalt utan att installera den i `~/.cargo/bin`:

```sh
cargo build --release
./target/release/shl_stats
```

Du kan också starta direkt från projektmappen med `cargo run`. Då bygger Cargo appen automatiskt vid behov.

## Flikar och navigering

Tryck `Tab` eller `→` för nästa flik och `←` för föregående. Flikarna är Matcher, Spelare, SHL-tabell och Målvakter. Använd `↑`/`↓` eller `k`/`j` för att flytta markeringen i listan.

Tryck `r` för att hämta om statistiken. Tryck `q` eller `Esc` för att avsluta när du inte skriver i ett sökfält.

## Matcher

Visar matcherna från de två senaste spelomgångarna och de två kommande omgångarna.

## Spelare

Visar spelare från alla SHL-lag med GP, mål, assist, poäng, +/-, utvisningsminuter (PIM), samt mål-, assist- och poängsnitt per match.

| Tangent | Funktion |
| --- | --- |
| `/` | Sök efter spelarnamn. Skriv söktext och tryck `Enter`. `Esc` rensar sökningen. |
| `l` | Filtrera på lag. Skriv hela eller en del av lagnamnet och tryck `Enter`. `Esc` rensar lagfiltret. |
| `g`, `a`, `p`, `x` | Sortera globalt på mål, assist, poäng eller +/-. |
| `i` | Sortera globalt på utvisningsminuter (PIM). |
| `m`, `b`, `e` | Sortera globalt på mål-, assist- eller poängsnitt per match. |
| Versalerna `G`, `A`, `P`, `X`, `I`, `M`, `B`, `E` | Sortera först alfabetiskt per lag och sedan på motsvarande statistik, inklusive PIM med `I`. |

Tryck samma sorteringstangent igen för att vända sorteringsordningen. Den aktiva sorteringskolumnen markeras i fetstil. Sökning och lagfilter kan användas tillsammans.

## SHL-tabell

Visar spelade matcher, vinster, oavgjorda, förluster, gjorda och insläppta mål, målskillnad, poäng och poäng per match.

| Tangent | Funktion |
| --- | --- |
| `p` | Sortera på totalpoäng. |
| `s` | Sortera på poäng per match. |

Tryck samma tangent igen för att vända sorteringsordningen.

## Målvakter

Visar spelade matcher (GP), skott emot, räddningsprocent, vinster och räddade skott.

| Tangent | Funktion |
| --- | --- |
| `g` | Sortera på spelade matcher. |
| `s` | Sortera på skott emot. |
| `v` | Sortera på räddningsprocent. |
| `w` | Sortera på vinster. |
| `c` | Sortera på räddade skott. |
| Versalerna `G`, `S`, `V`, `W`, `C` | Sortera först alfabetiskt per lag och sedan på statistiken. |

Tryck samma tangent igen för att vända sorteringsordningen. Den aktiva sorteringskolumnen markeras i fetstil.

## Datakälla

Statistiken hämtas från Svenska Ishockeyförbundets webbplats stats.swehockey.se. Programmet läser sajtens HTML, så ändringar i sidornas struktur kan kräva uppdateringar av parsern.
