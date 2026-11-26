import { useEffect, useState } from "react";
import type { AppInfo } from "../../types";
import { message } from "../../lib/errors";
import type { DownloadProgress } from "../../lib/updater";
import { runCheck, runInstall, runRelaunch, useUpdatePhase } from "../../state/useUpdate";

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
  const phase = useUpdatePhase();

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
