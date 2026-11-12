import { useEffect, useRef } from "react";
import type { Settings, SettingsPatch } from "../../types";
import { useSettings } from "../../state/useSettings";
import { NotificationsGroup } from "./NotificationsGroup";

interface Props {
  readonly load: () => Promise<Settings>;
  readonly save: (patch: SettingsPatch) => Promise<Settings>;
  readonly onTestAlert: () => Promise<void>;
  readonly onClose: () => void;
}

/** The app's settings, as a plate over the room. Changes save at once. */
export function SettingsPanel({ load, save, onTestAlert, onClose }: Props): React.ReactElement {
  const { state, change, retry } = useSettings(load, save);
  const done = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    done.current?.focus();
  }, []);

  return (
    <div
      className="dialog settings-dialog"
      role="dialog"
      aria-labelledby="settings-title"
      data-testid="settings-dialog"
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
      }}
    >
      <h2 id="settings-title">Settings</h2>
      <div className="settings-body">
        {state.status === "loading" ? (
          <p className="panel-hint" role="status">
            Reading the settings…
          </p>
        ) : null}
        {state.status === "error" ? (
          <div className="form-error" role="alert">
            Could not read the settings: {state.message}.{" "}
            <button type="button" className="link" onClick={retry}>
              Try again
            </button>
          </div>
        ) : null}
        {state.status === "ready" ? (
          <section className="settings-group">
            <h3>Notifications</h3>
            <NotificationsGroup value={state.settings.notifications} onChange={(notifications) => change({ notifications })} onTest={onTestAlert} />
          </section>
        ) : null}
      </div>
      {state.status === "ready" && state.saveError !== null ? (
        <p className="form-error" role="alert">
          Could not save that: {state.saveError}.
        </p>
      ) : null}
      <div className="dialog-actions">
        <button ref={done} type="button" className="btn btn-primary" data-testid="settings-close" onClick={onClose}>
          Done
        </button>
      </div>
    </div>
  );
}
