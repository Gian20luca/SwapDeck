import type { itInfo } from './it.info.js';

/** English texts of the «i» hints (InfoTip); must cover every key of `it.info.ts`. */
export const enInfo: Record<keyof typeof itInfo, string> = {
  'info.label': 'About {name}',

  'info.dashboard':
    'The Claude accounts managed by cswap, with the usage of each. The active account is the one Claude Code is using right now.',
  'info.usage.fiveHour':
    'How much you have used in the 5-hour window. At 100% Claude stops on this account until the reset shown under the bar.',
  'info.usage.sevenDay':
    'How much you have used of the weekly limit. The dashed mark is the even pace: where you would be if you used it at a steady rate until the reset.',
  'info.pace':
    'Compares this week’s usage with an even pace. If you are ahead, the limit may run out before the reset.',
  'info.usage.scoped':
    'Some models have their own weekly cap, on top of the general limits. Each one has its own reset.',

  'info.detail.name':
    'The alias is an easy-to-remember name instead of the email: you use it in the app and in cswap commands.',
  'info.detail.use':
    'Disabling takes the account out of the choice, the engine’s too, without deleting it. The terminal starts with this account without changing the active one.',
  'info.detail.slot':
    'The slot is the account’s number in cswap and sets its order. Moving or swapping only changes the numbers, not the credentials.',
  'info.detail.danger':
    'Removing takes the account out of cswap. To get it back you have to log in again.',

  'info.folders.list':
    'A mapped folder always uses the chosen account, even when the active one is another. Handy to keep a project on a specific account.',
  'info.folders.add':
    'Pick the account and the project folder: from then on Claude Code, started through cswap, uses that account there.',

  'info.history.range': 'The period shown in the charts and in the list of switches and decisions.',
  'info.history.charts':
    'The line is the real usage; the dashed line is the even pace until the reset. If the line is above, you are using more than expected.',
  'info.history.events':
    'A “Switch” is an account change that happened, by hand or by the engine. A “Decision” is what the engine evaluated, even when it chose not to switch.',

  'info.tools.export':
    'Saves the accounts to a file, to keep a copy or move them to another computer. The file holds the credentials in clear text: keep it safe.',
  'info.tools.import':
    'Reloads the accounts from a file exported earlier. Useful after a reinstall or on a new computer.',
  'info.tools.import.force':
    'If an account already exists in the same slot, the file replaces it. Without this option it is skipped.',
  'info.tools.unclaimed':
    'Credentials cswap has set aside without a linked account. You can delete them one by one to tidy up.',
  'info.tools.purge':
    'Deletes everything cswap has saved (accounts, credentials, session profiles). It does not touch the current Claude Code login and it cannot be undone.',

  'info.settings.app':
    'How SwapDeck looks and how often it reads usage. They apply to this app only, not to cswap.',
  'info.settings.language':
    'The language of SwapDeck’s texts. “Automatic” follows the system language.',
  'info.settings.theme': 'Light or dark look. “Automatic” follows the system theme.',
  'info.settings.interval':
    'How many seconds SwapDeck waits between usage reads. Low values keep the data fresher but make more reads.',
  'info.settings.thresholds':
    'When the active account’s usage goes above one of these percentages, you get a notification.',
  'info.settings.notif':
    'Choose which alerts to receive and at what times. If you do not want them all, turn them off one by one here.',
  'info.settings.autostart':
    'SwapDeck starts by itself when you sign in to the computer, so monitoring and the engine are always on.',
  'info.config':
    'Values read by cswap’s automatic switching (“cswap auto”), not by SwapDeck. Where no value is set, the default applies.',
  'info.config.threshold':
    'Usage percentage of the active account above which cswap moves to another one. The lower it is, the earlier it switches.',
  'info.config.intervalSeconds':
    'How many seconds cswap waits between usage checks to decide whether to switch.',
  'info.config.cooldownSeconds':
    'Minimum pause after a switch before another one is allowed. It avoids rapid-fire changes.',
  'info.config.hysteresisPct':
    'Margin, in percentage points below the threshold, that avoids switching back and forth between accounts close to the threshold. Higher values make switching steadier.',
  'info.config.strategy':
    '“best” picks the account with the most free headroom. “consume-first” uses first the one whose limit resets soonest, so nothing goes to waste.',
  'info.config.includeApiKeyAccounts':
    'API-key accounts are billed per use. By default automatic switching does not use them as a destination: turn this on only if you want it to.',
  'info.config.unhealthyTicks':
    'How many consecutive check rounds an account must fail before it is put in quarantine, that is left out of the choice. It avoids dropping an account for a passing error.',
  'info.config.model':
    'Which per-model limits (for example Fable) count in the choice. Names separated by commas, or “all” for every one; if empty, no model counts.',

  'info.notif.engine':
    'Alerts about the engine: when it is about to switch account, when it did, and when it has nowhere left to go.',
  'info.notif.usage':
    'Alerts about account usage and resets, so you notice in time that a limit is getting close.',
  'info.notif.state':
    'Alerts about the state of the app and of cswap: engine paused, login to redo, stale data, new versions.',
  'info.notif.quiet':
    'A time range without notifications, for example at night. Blocked alerts are dropped and do not arrive later.',
  'info.notif.summary':
    'A single notification a day with the picture of usage and switches, for those who do not want constant alerts.',
  'info.notif.switchPreview':
    'Gives you time to save your work before the engine switches account. It fires when the threshold is near, in points or in minutes.',
  'info.notif.noReserve':
    'It happens when every account is at its limit: Claude stops until one resets. It is the most urgent alert.',
  'info.notif.percent':
    'An alert each time usage goes above a threshold, for example 80 and 95. Thresholds are independent for the 5 hours and the 7 days.',
  'info.notif.exhaustion':
    'Estimates when the weekly limit will run out if you keep this pace, and warns you if that falls before the reset.',
  'info.notif.modelLimit':
    'The same threshold alerts, but for a model’s own limits, which run out separately.',
  'info.notif.engineState':
    'The engine stops by itself in some cases, for example when another engine is already active. The alert tells you when it stops and when it restarts.',
  'info.notif.login':
    'An account with an expired login stops working until you sign in again with /login. The alert comes before the expiry.',
  'info.notif.stale':
    'If usage stops updating, the bars show old numbers and the engine decides without fresh data.',
  'info.notif.quietCritical':
    'Critical alerts (no reserve, login to redo) need an action: to avoid missing them, let them through at night too.',
  'info.rules.state':
    'On: the engine switches account by itself, for every session. Observe only: it decides and logs it, but never switches. Off: it evaluates nothing. Suspended: it stopped by itself (another engine, invalid rules) and resumes when the problem is gone.',
  'info.rules.next':
    'The account the engine would move to if the active one reached the threshold right now, with the reason. Handy to test the rules without waiting.',
  'info.maps.map':
    'A map is a named bundle: account list, thresholds and criterion. You pick it; the engine never changes it by itself. On, observe only and ignored conflicts apply to every map.',
  'info.maps.preset':
    'Presets are always there and cannot be changed. «Clone as a personal map» makes a named copy you can edit, rename and delete. A preset uses the list of the map you pick it from.',
  'info.maps.box':
    'The rules of the active map: who to use (list), when to switch (thresholds) and where to go (criterion). They apply from the round after saving.',
  'info.rules.list':
    'Who the engine may use, and in what order. The first one is the primary: the engine starts there and, with «Return to the primary», goes back after its reset. Excluded accounts are never picked.',
  'info.rules.thresholds':
    'When to switch: past the 5-hour or 7-day threshold the active account gives way. After the reset it returns to the primary only if it is below the threshold by at least the hysteresis; the minimum pause avoids bursts of switches.',
  'info.rules.criterion':
    'Which reserve below the thresholds to move to. Lowest weekly; Best: most headroom; Fixed order: first in the list; Soonest reset; Pace-safe: lasts until the reset; Freest 5 hours; Balanced: weights; Even spread; Model headroom.',
  'info.rules.weights':
    'How much each part counts in the balanced score: 5-hour and 7-day headroom, closeness of the weekly reset, pace. Only the proportions matter; a factor without data is dropped.',
  'info.rules.model':
    'The weekly limit of a single model, for example Fable: the engine picks the reserve with the most headroom on that limit and, on a tie, the lowest weekly usage.',
  'info.rules.conflicts':
    'Another automation that switches account through cswap (for example cswap auto) can contradict this engine: if it is certain, SwapDeck pauses while it is there. «Ignore» lets it go on anyway.',

  'info.usage.what':
    'Claude Code usage on this PC, read from the saved sessions: by project, session, agent or model, credited to the account active at that time. It is measured in equivalent tokens, weighted like the API prices.',
  'info.usage.other':
    'How much the other devices (other PCs, web) use on the same account: the share read from the server minus the part explained by this PC’s tokens. Confidence grows with the number and consistency of the readings; at least 3 intervals are needed.',
  'info.usage.privacy':
    'Everything stays on this PC, in a local index with times, models and token counts. Message text is never read, folder paths never go to the logs and the app shows only the last folder name.',
  'info.usage.firstScan':
    'The first time SwapDeck reads the sessions of the last 30 days and builds an index; after that it reads only the new lines. If you switch tab, reading resumes where it stopped.',
};
