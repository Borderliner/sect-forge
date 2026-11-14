<p align="center">
  <img src="src/data/sect-forge.png" alt="Sect Forge lotus emblem" width="88">
</p>

<h1 align="center">Sect Forge</h1>

<p align="center"><strong>Create a playable faction for Total War: THREE KINGDOMS.</strong></p>

<p align="center">
  Windows desktop editor &nbsp;·&nbsp; 190 CE &nbsp;·&nbsp; Romance and Records &nbsp;·&nbsp; Vanilla and TROM 5.2
</p>

<p align="center">
  <a href="#quick-start">Quick start</a> &nbsp;·&nbsp;
  <a href="#play-a-sect-cult-or-warlord">Campaigns</a> &nbsp;·&nbsp;
  <a href="#build-from-source">Build from source</a> &nbsp;·&nbsp;
  <a href="#versioning">Versioning</a>
</p>

<table align="center">
  <tr>
    <td align="center"><strong>88</strong><br>Starting settlements</td>
    <td align="center"><strong>84</strong><br>Founder traits</td>
    <td align="center"><strong>24</strong><br>Warlord playstyles</td>
  </tr>
</table>

## Quick start

1. **Set your identity.** Launch `sect-forge.exe`. Sect Forge finds your Steam installation and Workshop packs. Use **Game folder…** to point it at another installation. Name your faction, choose its colours, and select **Sect**, **Cult**, or **Warlord**.
2. **Create a founder.** Pick a class, choose a vanilla or Workshop portrait, and set your starting traits and equipment. Each name has a **Reroll** button. The chosen portrait determines the given-name pool and titles like *Brotherhood* or *Sisterhood*.
3. **Pick a settlement.** Choose a marker on the 190 CE map or search by name, owner, or settlement type. The map shows each available start alongside the vanilla warlords.
4. **Export the pack.** Open **Review & export**, check the filename and campaign rules, and click **Export mod pack**. The default folder is `%LOCALAPPDATA%\SectForge\mods`. Enable the exported pack in your mod manager and start a fresh 190 CE campaign. **Save project…** stores your editable choices as JSON.

| Editor | What you can customize |
| --- | --- |
| **Identity** | Choose both faction colours from clickable swatches or enter their hex codes. |
| **Founder** | Set the founder’s name, class, and starting age (18–60) in **Personal details**. Browse playable male and female artwork in **Portrait**. |
| **Traits** | Browse 84 traits with their icons, gameplay effects, and scopes. Switch between List and Grid and filter by element, Good, or Bad. Choose up to three starting traits, including one physical trait. |
| **Equipment** | Browse all five equipment slots from the installed game and DLCs, with TROM gear available when compatibility is enabled. Compare combat stats and character bonuses, see Romance and Records differences, and check which items your founder can equip. |
| **Settlement** | Start at Hepu, one of 23 unoccupied sites, or one of 64 Han Empire settlements. The map displays faction colours and portraits for 24 vanilla warlords. |

Workshop packs can be found by filename or ID, or opened directly from disk. Complete image-only portraits can use their original artwork and unit card with a vanilla female 3D model. Exports that use Workshop artwork list the source pack as a dependency.

## Play a Sect, Cult, or Warlord

Choose **Warlord** under **Campaign rules** to browse 24 playstyles across the Han, Bandit, Yellow Turban, and Nanman families. The generated 190 CE campaign gives your founder a new faction with the selected warlord’s culture, recruitment permissions, reforms, building access, and faction rules. The historical warlord keeps their faction on the map. Warlord campaigns require the DLC that supplies the selected mechanics.

Warlord playstyles are experimental. Campaign starts have been checked through export in vanilla and TROM. Some faction-specific panels and scripted actions still need in-game verification.

| **Sect** | **Cult** |
| --- | --- |
| Grow Influence through doctrines and chapters. Collect donations, fund congregations, and send war chests to your patron. | Expand the Bandit Network, unlock capped recruits, and take timed raid contracts. |
| **36 reforms** across community, defence, and patronage. | **36 additional reforms** across raiding, recruitment, and stronghold development. |
| **Victory** when your chosen patron claims an imperial throne. | **Victory** after using **Sack and Withdraw** in 30 distinct commandery cities. |

For a **Sect**, envoys reveal the seats of active factions at the start. In **Diplomacy → Trade and Marriage**, offer **Make Regular Payments** as the only term. After the warlord accepts, confirm the recipient in **Choose a Patron** on your next turn. To switch patrons, make a fresh offer to another warlord and confirm them. Existing payment agreements continue under their original terms. Your sect wins if your current patron becomes Emperor or Empress. Endow up to five chapters. Each grants 6 Influence immediately and 1 Influence every three turns. Influence grants background income, diplomacy with all factions, faction-wide character satisfaction, and prestige at 25, 75, 150, 225, and 300. Sect ranks end at Duke or Duchess.

For a **Cult**, each new sack earns 5 Influence. Lose 1 per turn and 5 when a fence's contract expires. At 10, 25, and 50 Influence, gain +5%, +10%, and +20% settlement looting income. Market Fences can offer a five-turn contract after your first sack. Your armies lose campaign movement while the contract is open.

Sects can upgrade a **Meeting Hall** for recurring Sect Influence and culture income. Cults can upgrade a **Fences' Den** for banditry income and supply stockpiles. In TROM, build a **Wayside Shrine** or **Hidden Stash** in the attachment slot of an owned minor settlement.

### Sect Mode

Sect Mode is on by default for Sect and Cult campaigns. You can hold every settlement in one commandery. Acquiring territory in another moves your base and leaves your old holdings as ruins. **Supply Contracts** or **Smuggler Contracts** unlock upgrades that bypass material requirements for a higher construction cost while keeping settlement and slot requirements.

Turn Sect Mode off for unrestricted expansion and normal building resource requirements. Both settings use reforms to unlock advanced buildings and recruits. In TROM, a Sect using Sect Mode can research alternate access to selected standard elite units while keeping their class and skill requirements. Warlord campaigns default to unrestricted expansion and use their own building requirements in either setting.

### TROM 5.2

Install both TROM Workshop parts and select **TROM compatibility · 5.2** in **Review & export**. Enable the generated `!sects_<name>_trom.pack` above both parts in your mod manager. Sect Forge checks TROM Part 1's 190 CE start positions and places the faction in its selected settlement in both campaign modes.

Enable one generated faction pack at a time. Other mods that replace the same campaign start or research screen can conflict. Close the game before overwriting an existing pack.

## Build from source

Sect Forge is a native Windows app built with Rust and GPUI. Its embedded `rpfm_lib` writer works directly with your local Three Kingdoms installation and the bundled scenario and database definitions.

1. Install the stable Rust MSVC toolchain.
2. In Visual Studio Build Tools, select **Desktop development with C++**, the **MSVC x64/x86 build tools**, and a **Windows 10 or 11 SDK**.
3. Open **x64 Native Tools Command Prompt for VS** and run the following from the repository directory.

```bat
cargo build --locked --release --bins
.\target\release\sect-forge.exe
```

If `link.exe` is missing, reopen the x64 developer prompt after installing the MSVC tools. For a missing `kernel32.lib`, check that the Windows SDK is installed and that `LIB` includes its `Lib\<version>\um\x64` directory. [Microsoft C++ command-line setup](https://learn.microsoft.com/en-us/cpp/build/building-on-the-command-line)

<details>
<summary><strong>Command-line exports</strong></summary>

```powershell
.\target\release\sect-forge-cli.exe build configs\jade_crane_society.json
```

Use `--game` to specify the installation and `--output` to choose an export folder. Run `traits` or `portraits --pack <file>` to look up trait and portrait keys.

| JSON field | Value |
| --- | --- |
| `variant` | `"sect"`, `"cult"`, or `"warlord"`. Defaults to `"sect"`. |
| `warlord` | Source faction key for `"warlord"`, such as `"3k_main_faction_kong_rong"`. |
| `sectMode` | `true` for the one-commandery rules. Defaults to `true`. |
| `tromCompat` | `true` to build against TROM 5.2. |
| `founderAge` | Starting age in years, from 18 to 60. Defaults to 18 when omitted. |
| `artSet`, `portraitPack` | Portrait key and optional source pack path. |
| `subtype`, `startRegion` | Class and supported starting region keys, such as `3k_general_metal` and `3k_main_yizhou_island_capital`. |
| `startingTraits` | Array of trait keys, such as `3k_main_ceo_trait_personality_brave`. |
| `startingAncillaries` | Map of slot to item key. Save a project from the editor to get the keys for your selected equipment. |

The equipment map accepts `weapon`, `armour`, `mount`, `accessory`, and `follower`. Older projects can use `extra` for one accessory or follower. The Hepu scenario supplies campaign IDs and the base faction data.

</details>

## Versioning

The package version in [Cargo.toml](Cargo.toml#L3) drives the app footer and both executables’ `--version` output. Enable the Git hook once per clone with Git for Windows and PowerShell 7 on your PATH.

```powershell
git config --local core.hooksPath .githooks
```

The hook stages a **patch** bump on each commit by default and updates the app entry in `Cargo.lock`. For a compatible feature or a breaking change, choose a **minor** or **major** bump before committing. The hook uses the version you already staged. [Semantic Versioning](https://semver.org/) · [Git hooks](https://git-scm.com/docs/githooks)

```powershell
pwsh -File tools/version.ps1 minor  # Compatible feature
pwsh -File tools/version.ps1 major  # Breaking change
```

<details>
<summary><strong>Source and credits</strong></summary>

- `src/desktop.rs` and `src/desktop/` implement the editor. `src/main.rs` implements the CLI.
- `src/export.rs`, `src/startpos.rs`, and `src/trom.rs` write the pack and its campaign starts.
- `src/doctrines.rs`, `src/unlocks.rs`, and `src/script/` implement campaign progression.
- The bundled database definitions are from [RPFM Schemas](https://github.com/Frodo45127/rpfm-schemas) under the [MIT license](src/data/rpfm-schemas.LICENSE).

</details>
