// app.js — cc-speedy web view (vanilla JS, no build step).
// Exports `window.ccSpeedy` with `initDashboard`, `refreshDashboard`, and `initSession`.

(function () {
    "use strict";

    function escapeHtml(s) {
        return String(s)
            .replace(/&/g, "&amp;")
            .replace(/</g, "&lt;")
            .replace(/>/g, "&gt;")
            .replace(/"/g, "&quot;")
            .replace(/'/g, "&#039;");
    }

    function formatRelative(unixSecs) {
        const now = Math.floor(Date.now() / 1000);
        const delta = now - unixSecs;
        if (delta < 60) return delta + "s ago";
        if (delta < 3600) return Math.floor(delta / 60) + "m ago";
        if (delta < 86400) return Math.floor(delta / 3600) + "h ago";
        return Math.floor(delta / 86400) + "d ago";
    }

    function livenessGlyph(state) {
        if (state === "live") return '<span class="glyph-live">▶</span>';
        if (state === "recent") return '<span class="glyph-recent">◦</span>';
        return '<span class="glyph-idle">·</span>';
    }

    function badge(source) {
        const cls = "badge badge-" + source;
        return '<span class="' + cls + '">' + source.toUpperCase() + '</span>';
    }

    function renderSessionRow(s) {
        const truncatedSummary = (s.summary || s.first_user_msg || "")
            .slice(0, 80);
        return (
            '<a href="/session/' + encodeURIComponent(s.session_id) + '" class="session-row">' +
                livenessGlyph(s.liveness) +
                badge(s.source) +
                '<span class="summary">' + escapeHtml(truncatedSummary) + '</span>' +
                '<span class="path">' + escapeHtml(s.project_path) + '  ·  ' + formatRelative(s.modified_unix_secs) + '</span>' +
            '</a>'
        );
    }

    async function refreshDashboard() {
        const root = document.getElementById("dashboard");
        if (!root) return;
        try {
            const resp = await fetch("/api/sessions");
            if (!resp.ok) {
                root.innerHTML = "Error: " + resp.status;
                return;
            }
            const sessions = await resp.json();
            const groups = { cc: [], oc: [], co: [] };
            for (const s of sessions) {
                if (groups[s.source]) groups[s.source].push(s);
            }
            const sectionTitles = { cc: "Claude Code", oc: "OpenCode", co: "Copilot" };
            let html = "";
            for (const k of ["cc", "oc", "co"]) {
                if (groups[k].length === 0) continue;
                html += '<div class="section">';
                html += '<h2 class="section-header">' + sectionTitles[k] + " (" + groups[k].length + ")</h2>";
                html += groups[k].map(renderSessionRow).join("");
                html += "</div>";
            }
            if (html === "") html = "<p>No sessions found.</p>";
            root.innerHTML = html;
        } catch (e) {
            root.innerHTML = "Error: " + escapeHtml(e.message);
        }
    }

    function initDashboard() {
        refreshDashboard();
    }

    function renderTurn(turn) {
        let html = '<div class="turn">';
        if (turn.user_msg) {
            html += '<div class="turn-user"><strong>USER:</strong> ' + escapeHtml(turn.user_msg) + '</div>';
        }
        html += '<div class="turn-assistant">';
        for (const block of (turn.blocks || [])) {
            if (block.kind === "text") {
                html += '<div>' + escapeHtml(block.text || "") + '</div>';
            } else if (block.kind === "thinking") {
                html += '<details><summary>thinking</summary><pre>' + escapeHtml(block.text || "") + '</pre></details>';
            } else if (block.kind === "tool_use") {
                html += '<div class="tool-use"><strong>tool: ' + escapeHtml(block.name || "") + '</strong>';
                if (block.input_json) html += '<pre>' + escapeHtml(block.input_json) + '</pre>';
                html += '</div>';
            } else if (block.kind === "tool_result") {
                html += '<details><summary>tool result' + (block.is_error ? " (error)" : "") + '</summary>';
                html += '<pre>' + escapeHtml(block.text || "") + '</pre></details>';
            }
        }
        html += '</div></div>';
        return html;
    }

    async function initSession() {
        const sessionId = location.pathname.split("/").pop();
        const titleEl = document.getElementById("session-title");
        const turnsEl = document.getElementById("turns");
        const liveBadgeEl = document.getElementById("live-badge");
        if (!sessionId || !turnsEl) return;

        try {
            const resp = await fetch("/api/sessions");
            const sessions = await resp.json();
            const session = sessions.find(s => s.session_id === sessionId);
            if (session) {
                titleEl.firstChild.textContent = session.summary || session.first_user_msg || sessionId;
                liveBadgeEl.textContent = session.liveness === "live" ? "▶ live" : (session.liveness === "recent" ? "◦ recent" : "○ idle");
                liveBadgeEl.className = "live-badge" + (session.liveness === "live" ? "" : " disconnected");

                turnsEl.innerHTML = "";
                let renderedAny = false;
                for (let i = 0; i < 200; i++) {
                    try {
                        const tResp = await fetch("/api/session/" + encodeURIComponent(sessionId) + "/turns/" + i);
                        if (!tResp.ok) break;
                        const turn = await tResp.json();
                        turnsEl.insertAdjacentHTML("beforeend", renderTurn(turn));
                        renderedAny = true;
                    } catch (e) {
                        break;
                    }
                }
                if (!renderedAny) {
                    turnsEl.textContent = "No turns yet.";
                }

                if (session.liveness === "live") {
                    openStream(sessionId, turnsEl, liveBadgeEl);
                }
            } else {
                titleEl.firstChild.textContent = "Unknown session";
            }
        } catch (e) {
            turnsEl.innerHTML = "Error loading session: " + escapeHtml(e.message);
        }
    }

    function openStream(sessionId, turnsEl, liveBadgeEl) {
        const url = "/session/" + encodeURIComponent(sessionId) + "/stream";
        const es = new EventSource(url);
        es.addEventListener("turn-added", async (e) => {
            try {
                const data = JSON.parse(e.data);
                const idx = data.idx;
                const tResp = await fetch("/api/session/" + encodeURIComponent(sessionId) + "/turns/" + idx);
                if (tResp.ok) {
                    const turn = await tResp.json();
                    turnsEl.insertAdjacentHTML("beforeend", renderTurn(turn));
                    window.scrollTo(0, document.body.scrollHeight);
                }
            } catch (err) { /* ignore */ }
        });
        es.addEventListener("turn-updated", async (e) => {
            await initSession();
        });
        es.addEventListener("liveness", (e) => {
            try {
                const data = JSON.parse(e.data);
                liveBadgeEl.textContent = data.state === "live" ? "▶ live" : (data.state === "recent" ? "◦ recent" : "○ idle");
                liveBadgeEl.className = "live-badge" + (data.state === "live" ? "" : " disconnected");
                if (data.state !== "live") {
                    es.close();
                }
            } catch (err) { /* ignore */ }
        });
        es.onerror = () => {
            liveBadgeEl.className = "live-badge disconnected";
        };
    }

    window.ccSpeedy = {
        initDashboard,
        refreshDashboard,
        initSession,
    };
})();
