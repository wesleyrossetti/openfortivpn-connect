import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CertificateTokenSuggestion, VpnProfile } from "../types";
import { newProfile } from "../types";
import { TrustedCertManager } from "./TrustedCertManager";
import { DeleteConfirmModal } from "./DeleteConfirmModal";

interface Props {
  profile: VpnProfile | null; // null = creating new
  onSave: (profile: VpnProfile, password?: string, tokenPin?: string) => Promise<void>;
  onCancel: () => void;
  onDelete?: (id: string) => Promise<void>;
}

export function ProfileEditor({ profile, onSave, onCancel, onDelete }: Props) {
  const isNew = profile === null;
  const [form, setForm] = useState<VpnProfile>(profile ?? newProfile());
  const [password, setPassword] = useState("");
  const [tokenPin, setTokenPin] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [tokenSuggestions, setTokenSuggestions] = useState<CertificateTokenSuggestion[]>([]);
  const [tokenStatus, setTokenStatus] = useState<string | null>(null);
  const [loadingTokens, setLoadingTokens] = useState(false);
  const autoSuggestedRef = useRef(false);

  const detectCertificateTokens = async (autoFill: boolean) => {
    setLoadingTokens(true);
    setTokenStatus(null);
    try {
      const suggestions = await invoke<CertificateTokenSuggestion[]>("suggest_certificate_tokens");
      setTokenSuggestions(suggestions);

      if (suggestions.length === 0) {
        setTokenStatus("No PKCS#11 token URI detected in the current session.");
        return;
      }

      if (autoFill) {
        setForm((current) => {
          if (current.auth_type !== "CertificateToken" || (current.user_cert ?? "").trim()) {
            return current;
          }
          autoSuggestedRef.current = true;
          return {
            ...current,
            user_cert: suggestions[0].uri,
            pkcs11_provider: suggestions[0].provider,
          };
        });
      }

      setTokenStatus(`${suggestions.length} token suggestion${suggestions.length === 1 ? "" : "s"} found.`);
    } catch (e) {
      setTokenSuggestions([]);
      setTokenStatus(String(e));
    } finally {
      setLoadingTokens(false);
    }
  };

  useEffect(() => {
    if (form.auth_type !== "CertificateToken") {
      return;
    }

    if (autoSuggestedRef.current) {
      return;
    }

    void detectCertificateTokens(true);
  }, [form.auth_type]);

  const handleSave = async () => {
    if (!form.name.trim() || !form.host.trim()) {
      setError("Name and Host are required");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await onSave(form, password || undefined, tokenPin || undefined);
    } catch (e) {
      setError(String(e));
      setSaving(false);
    }
  };

  const inputClass =
    "w-full bg-white/5 border border-white/10 rounded-lg px-3 py-1.5 text-sm text-white/90 placeholder-white/30 focus:outline-none focus:border-blue-500/50 focus:ring-1 focus:ring-blue-500/30";

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-2 mb-1">
        <button
          onClick={onCancel}
          className="text-white/40 hover:text-white/80 transition-colors"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
          </svg>
        </button>
        <h2 className="text-sm font-semibold text-white/80">
          {isNew ? "New Profile" : "Edit Profile"}
        </h2>
        {!isNew && onDelete && (
          <button
            onClick={() => setShowDeleteConfirm(true)}
            className="ml-auto text-red-400 hover:text-red-300 text-xs transition-colors"
          >
            Delete
          </button>
        )}
      </div>

      {error && (
        <div className="text-sm text-red-400 bg-red-500/10 border border-red-500/20 rounded-lg px-3 py-2">{error}</div>
      )}

      <div className="flex flex-col gap-2.5 max-h-72 overflow-y-auto pr-1">
        <label className="flex flex-col gap-1">
          <span className="text-xs text-white/40">Name</span>
          <input
            className={inputClass}
            value={form.name}
            onChange={(e) => setForm({ ...form, name: e.target.value })}
            placeholder="My VPN"
          />
        </label>

        <div className="grid grid-cols-3 gap-2">
          <label className="flex flex-col gap-1 col-span-2">
            <span className="text-xs text-white/40">Host</span>
            <input
              className={inputClass}
              value={form.host}
              onChange={(e) => setForm({ ...form, host: e.target.value })}
              placeholder="vpn.example.com"
            />
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-xs text-white/40">Port</span>
            <input
              type="number"
              className={inputClass}
              value={form.port}
              onChange={(e) => setForm({ ...form, port: parseInt(e.target.value) || 8443 })}
            />
          </label>
        </div>

        <div className="flex flex-col gap-1">
          <span className="text-xs text-white/40">Authentication</span>
          <div className="flex gap-4">
            <label className="flex items-center gap-1.5 cursor-pointer">
              <input
                type="radio"
                name="auth_type"
                checked={form.auth_type === "Password"}
                onChange={() => setForm({ ...form, auth_type: "Password" })}
                className="accent-blue-500"
              />
              <span className="text-sm text-white/70">Password</span>
            </label>
            <label className="flex items-center gap-1.5 cursor-pointer">
              <input
                type="radio"
                name="auth_type"
                checked={form.auth_type === "Saml"}
                onChange={() => setForm({ ...form, auth_type: "Saml" })}
                className="accent-blue-500"
              />
              <span className="text-sm text-white/70">SAML</span>
            </label>
            <label className="flex items-center gap-1.5 cursor-pointer">
              <input
                type="radio"
                name="auth_type"
                checked={form.auth_type === "CertificateToken"}
                onChange={() => setForm({ ...form, auth_type: "CertificateToken" })}
                className="accent-blue-500"
              />
              <span className="text-sm text-white/70">Cert/Token</span>
            </label>
          </div>
        </div>

        {form.auth_type === "Password" && (
          <>
            <label className="flex flex-col gap-1">
              <span className="text-xs text-white/40">Username</span>
              <input
                className={inputClass}
                value={form.username ?? ""}
                onChange={(e) => setForm({ ...form, username: e.target.value || null })}
                placeholder="john.doe"
              />
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-xs text-white/40">Password</span>
              <input
                type="password"
                className={inputClass}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder={isNew ? "Enter password" : "Leave empty to keep current"}
              />
            </label>
          </>
        )}

        {form.auth_type === "CertificateToken" && (
          <>
            <label className="flex flex-col gap-1">
              <span className="text-xs text-white/40">Username (optional)</span>
              <input
                className={inputClass}
                value={form.username ?? ""}
                onChange={(e) => setForm({ ...form, username: e.target.value || null })}
                placeholder="john.doe"
              />
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-xs text-white/40">Certificate / Token URI</span>
              <input
                className={inputClass}
                value={form.user_cert ?? ""}
                onChange={(e) => setForm({ ...form, user_cert: e.target.value || null })}
                placeholder="pkcs11:token=MyToken;id=%01 or /path/to/cert.pem"
              />
              <span className="text-xs text-white/30">
                Use a PKCS#11 token URI narrowed by token and key ID, or a certificate path accepted by <code>openfortivpn</code>.
              </span>
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-xs text-white/40">PKCS#11 Provider (optional)</span>
              <input
                className={inputClass}
                value={form.pkcs11_provider ?? ""}
                onChange={(e) => setForm({ ...form, pkcs11_provider: e.target.value || null })}
                placeholder="/usr/lib64/libeToken.so"
              />
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-xs text-white/40">Token PIN</span>
              <input
                type="password"
                className={inputClass}
                value={tokenPin}
                onChange={(e) => setTokenPin(e.target.value)}
                placeholder={isNew ? "Enter token PIN" : "Leave empty to keep current"}
              />
              <span className="text-xs text-white/30">
                The PIN is stored separately from the profile and injected into the PKCS#11 URI only at connect time.
              </span>
            </label>
            <div className="rounded-lg border border-white/10 bg-white/5 px-3 py-2">
              <div className="flex items-center justify-between gap-3">
                <span className="text-xs text-white/50">Detected token suggestions</span>
                <button
                  type="button"
                  onClick={() => void detectCertificateTokens(false)}
                  disabled={loadingTokens}
                  className="text-xs text-blue-300 hover:text-blue-200 disabled:opacity-50 transition-colors"
                >
                  {loadingTokens ? "Scanning..." : "Rescan"}
                </button>
              </div>
              {tokenSuggestions.length > 0 ? (
                <div className="mt-2 flex flex-col gap-2">
                  {tokenSuggestions.map((suggestion) => (
                    <button
                      key={`${suggestion.provider}:${suggestion.uri}`}
                      type="button"
                      onClick={() =>
                        setForm({
                          ...form,
                          user_cert: suggestion.uri,
                          pkcs11_provider: suggestion.provider,
                        })
                      }
                      className="rounded-md border border-white/10 bg-black/20 px-2.5 py-2 text-left hover:border-blue-400/40 hover:bg-blue-500/10 transition-colors"
                    >
                      <div className="text-sm text-white/80">{suggestion.display_name}</div>
                      <div className="text-xs text-white/40 break-all">{suggestion.uri}</div>
                    </button>
                  ))}
                </div>
              ) : (
                <div className="mt-2 text-xs text-white/35">
                  {tokenStatus ?? "Select Cert/Token to scan known PKCS#11 providers."}
                </div>
              )}
            </div>
          </>
        )}

        <label className="flex flex-col gap-1">
          <span className="text-xs text-white/40">Realm (optional)</span>
          <input
            className={inputClass}
            value={form.realm ?? ""}
            onChange={(e) => setForm({ ...form, realm: e.target.value || null })}
            placeholder="optional"
          />
        </label>

        <TrustedCertManager
          certs={form.trusted_certs}
          onChange={(certs) => setForm({ ...form, trusted_certs: certs })}
        />
      </div>

      <div className="flex gap-2 mt-1">
        <button
          onClick={handleSave}
          disabled={saving}
          className="flex-1 py-2 px-4 bg-blue-500 hover:bg-blue-600 disabled:opacity-50 text-white text-sm font-medium rounded-lg transition-colors"
        >
          {saving ? "Saving..." : "Save"}
        </button>
        <button
          onClick={onCancel}
          className="flex-1 py-2 px-4 bg-white/10 hover:bg-white/15 text-white/70 text-sm font-medium rounded-lg transition-colors"
        >
          Cancel
        </button>
      </div>

      {/* Delete Confirmation Modal */}
      {showDeleteConfirm && onDelete && (
        <DeleteConfirmModal
          profileName={form.name || "Unnamed"}
          onConfirm={() => onDelete(form.id)}
          onCancel={() => setShowDeleteConfirm(false)}
        />
      )}
    </div>
  );
}
