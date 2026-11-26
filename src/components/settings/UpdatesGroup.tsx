import { useCallback, useEffect, useState } from "react";
import type { AppInfo } from "../../types";
import { message } from "../../lib/errors";
import { checkForUpdate, relaunchApp, type AvailableUpdate, type DownloadProgress } from "../../lib/updater";

type Phase =
  | { readonly kind: "idle" }
  | { readonly kind: "checking" }
  | { readonly kind: "latest" }
  | { readonly kind: "available"; readonly update: AvailableUpdate }
  | { readonly kind: "downloading"; readonly update: AvailableUpdate; readonly progress: DownloadProgress | null }
  | { readonly kind: "installed"; readonly version: string }
  | { readonly kind: "failed"; readonly text: string; readonly retry: "check" | "install" | "relaunch"; readonly update: AvailableUpdate | null };

interface Props {
  readonly loadInfo: () => Promise<AppInfo>;
}

const SHORT_COMMIT = 7;
const FAILURE = { check: "Could not check for updates", install: "Could not install the update", relaunch: "Could not relaunch" } as const;

function versionLine(info: AppInfo): string {
  return `Version ${info.version} (${info.commit.slice(0, SHORT_COMMIT)})`;
}

function percent(progress: DownloadProgress | null): string {
  if (progress === null || progress.total === null || progress.total === 0) return "";
  return ` ${Math.min(100, Math.floor((progress.received / progress.total) * 100))}%`;
}

/** The installed version, and a button that looks for a newer release on GitHub. */
export function UpdatesGroup({ loadInfo }: Props): React.ReactElement {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [infoError, setInfoError] = useState<string | null>(null);
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });

  useEffect(() => {
    let live = true;
    loadInfo().then(
      (loaded) => {
        if (live) setInfo(loaded);
      },
      (err: unknown) => {
        if (live) setInfoError(message(err));
      },
    );
    return () => {
      live = false;
    };
  }, [loadInfo]);

  const runCheck = useCallback((): void => {
    setPhase({ kind: "checking" });
    checkForUpdate().then(
      (update) => setPhase(update === null ? { kind: "latest" } : { kind: "available", update }),
      (err: unknown) => setPhase({ kind: "failed", text: message(err), retry: "check", update: null }),
    );
  }, []);

  const runInstall = useCallback((update: AvailableUpdate): void => {
    setPhase({ kind: "downloading", update, progress: null });
    update
      .install((progress) => setPhase({ kind: "downloading", update, progress }))
      .then(
        () => setPhase({ kind: "installed", version: update.version }),
        (err: unknown) => setPhase({ kind: "failed", text: message(err), retry: "install", update }),
      );
  }, []);

  const runRelaunch = useCallback((): void => {
    relaunchApp().catch((err: unknown) => setPhase({ kind: "failed", text: message(err), retry: "relaunch", update: null }));
  }, []);

  const busy = phase.kind === "checking" || phase.kind === "downloading";

  return (
    <>
      <p className="panel-hint" data-testid="settings-version">
        {info !== null ? versionLine(info) : infoError !== null ? `The version could not be read: ${infoError}.` : "Reading the version…"}
      </p>
      <div className="settings-test">
        <button type="button" className="btn" data-testid="settings-check-updates" disabled={busy || phase.kind === "installed"} onClick={runCheck}>
          Check for updates
        </button>
        {phase.kind === "checking" ? (
          <p className="panel-hint" role="status">
            Looking for a newer version…
          </p>
        ) : null}
        {phase.kind === "latest" ? (
          <p className="panel-hint" role="status">
            You have the latest version{info === null ? "" : ` (${info.version})`}.
          </p>
        ) : null}
        {phase.kind === "available" ? (
          <>
            <p className="panel-hint" role="status">
              Version {phase.update.version} is available.
            </p>
            {phase.update.notes === null ? null : <p className="panel-hint">{phase.update.notes}</p>}
            <button type="button" className="btn btn-primary" data-testid="settings-install-update" onClick={() => runInstall(phase.update)}>
              Install
            </button>
          </>
        ) : null}
        {phase.kind === "downloading" ? (
          <p className="panel-hint" role="status">
            Downloading version {phase.update.version}…{percent(phase.progress)}
          </p>
        ) : null}
        {phase.kind === "installed" ? (
          <>
            <p className="panel-hint" role="status">
              Version {phase.version} is installed. Relaunch when you are ready, after you have finished in any open terminals.
            </p>
            <button type="button" className="btn btn-primary" data-testid="settings-relaunch" onClick={runRelaunch}>
              Relaunch now
            </button>
          </>
        ) : null}
        {phase.kind === "failed" ? (
          <div className="form-error" role="alert">
            {FAILURE[phase.retry]}: {phase.text}.{" "}
            <button
              type="button"
              className="link"
              onClick={() => {
                if (phase.retry === "relaunch") runRelaunch();
                else if (phase.retry === "install" && phase.update !== null) runInstall(phase.update);
                else runCheck();
              }}
            >
              Try again
            </button>
          </div>
        ) : null}
      </div>
    </>
  );
}
