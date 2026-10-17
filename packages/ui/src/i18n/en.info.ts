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
    'Credentials left in cswap without a linked account, usually after a removal. You can delete them to tidy up.',
  'info.tools.purge':
    'Wipes everything cswap has saved. Use it to start from scratch or before uninstalling; it cannot be undone.',

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
    'Margin in percentage points that avoids switching back and forth: an account comes back into play only once it is far enough below the threshold.',
  'info.config.strategy':
    '“best” picks the account with the most free headroom. “consume-first” uses first the one whose limit resets soonest, so nothing goes to waste.',
  'info.config.includeApiKeyAccounts':
    'API-key accounts are billed per use. By default automatic switching does not use them: turn this on only if you want it to.',
  'info.config.unhealthyTicks':
    'How many checks in a row must fail before an account is set aside (quarantine). It avoids dropping an account for a passing error.',
  'info.config.model':
    'Which per-model limits (for example Fable) count in the choice. Names separated by commas, or “all” for every one.',
  'info.config.theme':
    'The theme of cswap’s terminal interface. It does not change how SwapDeck looks.',

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
};
