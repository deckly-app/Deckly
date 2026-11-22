# Pinned actions

Input to the pinned actions in `deckly-core` (`Profile::pinned`,
`Profile::resolve`, `Profile::pin`, `Profile::unpin`) and the editor.

- Date: 2026-11-22.
- Sources and method:
  - Web searches over Elgato's public pages about the official app: the help
    center articles on pinned actions and folders and on the 6.6 release
    notes and 6.6 beta changelog (help.elgato.com refused our fetcher, so for
    these only the search engine's summary was read), and Elgato's
    "Explorer" article on pinned actions, read as a web page.
  - Community sources (Reddit, forums, videos) were searched; none were found.
  - Not used: the official app itself (not run), its files, or any asset.
- Device: none. Official app: not run.
- Checked by: `deckly-core` profile tests (`a_pinned_key_shows_on_every_page_*`,
  `a_key_of_its_own_wins_*`, `pinning_refuses_*`, `a_pinned_folder_*`,
  `pins_belong_to_their_profile`, `pinned_keys_round_trip_*`,
  `damaged_pinned_keys_*`), runtime test
  `a_pinned_key_stays_one_instance_across_pages_and_folders`, editor tests
  `pin_override_and_unpin_are_undone_and_redone` and
  `the_inspector_edits_the_pin_under_an_empty_key`.

## Observed (from the sources above)

- The feature is called Pinned Actions (and folders) and needs Stream Deck
  6.6 or newer (spring 2024).
- Pins belong to one profile. A pinned action shows at the same key position
  on every page and in every folder of that profile.
- To pin: right-click a key and turn on Pin Action, or drop actions on a
  separate Pinned page placed left of the page numbers. Unpin from the same
  menu; the action then lands on the page or folder that is open.
- A page or folder that has its own action at that position shows its own
  action; the pin stays hidden there and shows wherever the position is empty.
  Dropping another action on a pinned one replaces it on that page only.
- Editing a pinned action changes it everywhere it shows (secondhand, beta
  changelog summary).
- Pinned actions look slightly grayed out in the editor.
- Folders can be pinned.

## Inferred

- Pins of one profile do not reach other profiles, devices or models.
- A pinned plugin action is one instance: moving between pages that show it
  is no appear/disappear for its plugin. Deckly gives it the context of the
  pinned page, so this holds by construction.

## Unknown (to check in the official app on Windows)

- Which actions cannot be pinned. Deckly refuses only the Parent Folder key,
  which belongs to its folder.
- Dials and the touch strip of Stream Deck +.
- Whether a blank placeholder action counts as the page's own action.
- The plugin events a pinned action gets when pages or profiles switch.
- Pinned actions in exported profiles, and where the official profile format
  keeps them (Deckly's import of official profiles leaves them out for now).
- What Unpin does when the open page already has an action at that position
  (Deckly refuses and says why).
- Whether the device shows any marker on a pinned key (Deckly shows none).
