import { AnimatePresence, motion } from "framer-motion";

export interface Toast {
  readonly id: number;
  readonly tone: "ok" | "error";
  readonly text: string;
}

/** Outcome messages, top-left over the scene, announced to screen readers. */
export function Toasts({ toasts, onDismiss }: { readonly toasts: readonly Toast[]; readonly onDismiss: (id: number) => void }): React.ReactElement {
  return (
    <div className="toasts" role="status" aria-live="polite">
      <AnimatePresence>
        {toasts.map((t) => (
          <motion.div
            key={t.id}
            className={`toast toast-${t.tone}`}
            data-testid="toast"
            initial={{ y: 12, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.16, ease: "easeOut" }}
          >
            <span>{t.text}</span>
            <button type="button" className="link" onClick={() => onDismiss(t.id)}>
              Dismiss
            </button>
          </motion.div>
        ))}
      </AnimatePresence>
    </div>
  );
}
