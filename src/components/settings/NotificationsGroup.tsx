import { useEffect, useRef, useState } from "react";
import type { NotificationSettings } from "../../types";
import { message } from "../../lib/errors";
import { COOLDOWN_MAX_SECS } from "../../lib/cooldown";
import { Toggle } from "../Toggle";

type SwitchKey = "finished" | "needsYou" | "stuck" | "sound" | "dockBounce" | "systemNotification";
interface SwitchRow {
  readonly key: SwitchKey;
  readonly label: string;
}

const EVENTS: readonly SwitchRow[] = [
  { key: "finished", label: "Finishes its turn" },
  { key: "needsYou", label: "Needs you" },
  { key: "stuck", label: "Is stuck on an error" },
];
const METHODS: readonly SwitchRow[] = [
  { key: "sound", label: "Sound" },
  { key: "dockBounce", label: "Dock bounce" },
  { key: "systemNotification", label: "System notification" },
];

const TEST_DELAY_MS = 3000;
const COOLDOWN_ERROR = `Enter a whole number of seconds from 0 to ${COOLDOWN_MAX_SECS}.`;

interface Props {
  readonly value: NotificationSettings;
  readonly onChange: (next: NotificationSettings) => void;
  readonly onTest: () => Promise<void>;
}

/** The whole seconds in `text`, 0 to 600, or null. */
function parseCooldown(text: string): number | null {
  if (!/^\d+$/.test(text.trim())) return null;
  const n = Number(text.trim());
  return n <= COOLDOWN_MAX_SECS ? n : null;
}

/** When and how the building tells you an agent stopped. */
export function NotificationsGroup({ value, onChange, onTest }: Props): React.ReactElement {
  const [text, setText] = useState(String(value.cooldownSecs));
  const [cooldownError, setCooldownError] = useState(false);
  const [waiting, setWaiting] = useState(false);
  const [testError, setTestError] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    setText(String(value.cooldownSecs));
    setCooldownError(false);
  }, [value.cooldownSecs]);

  useEffect(
    () => () => {
      if (timer.current !== null) clearTimeout(timer.current);
    },
    [],
  );

  const commit = (): void => {
    const secs = parseCooldown(text);
    if (secs === null) {
      setCooldownError(true);
      return;
    }
    setCooldownError(false);
    if (secs !== value.cooldownSecs) onChange({ ...value, cooldownSecs: secs });
  };

  const test = (): void => {
    setTestError(null);
    setWaiting(true);
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = setTimeout(() => setWaiting(false), TEST_DELAY_MS);
    onTest().catch((err: unknown) => {
      if (timer.current !== null) clearTimeout(timer.current);
      setTestError(message(err));
      setWaiting(false);
    });
  };

  const fieldset = (legend: string, rows: readonly SwitchRow[]): React.ReactElement => (
    <fieldset className="choice">
      <legend>{legend}</legend>
      {rows.map((row) => (
        <Toggle key={row.key} label={row.label} checked={value[row.key]} disabled={false} onChange={(on) => onChange({ ...value, [row.key]: on })} />
      ))}
    </fieldset>
  );

  return (
    <>
      {fieldset("Tell me when an agent", EVENTS)}
      {fieldset("How", METHODS)}
      <p className="panel-hint">The Dock bounce and the system notification only happen while another app is in front. You are never alerted about the desk whose panel you have open.</p>
      <label className="field">
        <span>At most one alert every</span>
        <span className="settings-cooldown">
          <input
            type="number"
            data-testid="settings-cooldown"
            min={0}
            max={COOLDOWN_MAX_SECS}
            step={1}
            inputMode="numeric"
            value={text}
            onChange={(e) => setText(e.target.value)}
            onBlur={commit}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                commit();
              }
            }}
          />
          seconds
        </span>
      </label>
      {cooldownError ? (
        <p className="form-error" role="alert">
          {COOLDOWN_ERROR}
        </p>
      ) : null}
      <p className="panel-hint">Alerts that arrive sooner come together, as one.</p>
      <div className="settings-test">
        <button type="button" className="btn" data-testid="settings-test" disabled={waiting} onClick={test}>
          Test in 3 seconds
        </button>
        {waiting ? <p className="panel-hint">Switch to another app now to see the notification and the Dock bounce.</p> : null}
        {testError === null ? null : (
          <p className="form-error" role="alert">
            Could not send the test: {testError}.
          </p>
        )}
      </div>
    </>
  );
}
