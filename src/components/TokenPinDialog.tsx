import { useEffect, useState } from "react";

interface Props {
  profileName: string;
  isOpen: boolean;
  onSubmit: (pin: string) => Promise<void>;
  onCancel: () => void;
}

export function TokenPinDialog({
  profileName,
  isOpen,
  onSubmit,
  onCancel,
}: Props) {
  const [pin, setPin] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (!isOpen) {
      setPin("");
      setError(null);
      setSubmitting(false);
    }
  }, [isOpen]);

  if (!isOpen) {
    return null;
  }

  const handleSubmit = async () => {
    if (!pin.trim()) {
      setError("Token PIN is required");
      return;
    }

    setSubmitting(true);
    setError(null);

    try {
      await onSubmit(pin);
    } catch (e) {
      setError(String(e));
      setSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/65 backdrop-blur-sm px-4">
      <div className="w-full max-w-sm rounded-2xl border border-white/10 bg-[#111418]/95 shadow-2xl">
        <div className="px-5 py-4 border-b border-white/10">
          <h2 className="text-base font-semibold text-white/90">Token PIN</h2>
          <p className="mt-1 text-sm text-white/45">
            Enter the PIN for <span className="text-white/70">{profileName}</span>.
          </p>
        </div>

        <div className="px-5 py-4">
          <label className="flex flex-col gap-2">
            <span className="text-xs uppercase tracking-wide text-white/35">PIN</span>
            <input
              type="password"
              autoFocus
              value={pin}
              onChange={(e) => setPin(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  void handleSubmit();
                }
              }}
              className="w-full rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white/90 placeholder-white/25 focus:outline-none focus:border-blue-500/50 focus:ring-1 focus:ring-blue-500/30"
              placeholder="Enter token PIN"
            />
          </label>

          {error && (
            <div className="mt-3 rounded-lg border border-red-500/20 bg-red-500/10 px-3 py-2 text-sm text-red-300">
              {error}
            </div>
          )}
        </div>

        <div className="flex gap-2 px-5 py-4 border-t border-white/10">
          <button
            onClick={handleSubmit}
            disabled={submitting}
            className="flex-1 rounded-lg bg-blue-500 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-600 disabled:opacity-50"
          >
            {submitting ? "Connecting..." : "Connect"}
          </button>
          <button
            onClick={onCancel}
            disabled={submitting}
            className="flex-1 rounded-lg bg-white/10 px-4 py-2 text-sm font-medium text-white/75 transition-colors hover:bg-white/15 disabled:opacity-50"
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
