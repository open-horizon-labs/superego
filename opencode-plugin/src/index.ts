/**
 * Superego OpenCode Plugin
 *
 * Metacognitive advisor for OpenCode. Injects contract on session start,
 * evaluates via OpenCode's configured LLM on session idle.
 */

import type { Plugin } from "@opencode-ai/plugin";
import { tool } from "@opencode-ai/plugin";
import { existsSync, readFileSync, mkdirSync, writeFileSync, rmSync, unlinkSync, appendFileSync } from "fs";
import { join } from "path";

// Log to file since OpenCode is a TUI
function log(superegoDir: string, message: string): void {
  const timestamp = new Date().toISOString();
  const logFile = join(superegoDir, "hook.log");
  try {
    appendFileSync(logFile, `${timestamp} ${message}\n`);
  } catch {
    // Ignore log failures
  }
}

const SUPEREGO_DIR = ".superego";
const BUILD_VERSION = `${new Date().toISOString().slice(0, 16)}`; // Build timestamp
const PROMPT_URL = "https://raw.githubusercontent.com/cloud-atlas-ai/superego/main/default_prompt.md";
const FALLBACK_PROMPT = `# Superego System Prompt

You are **Superego**, a metacognitive advisor. Respond with:

DECISION: [ALLOW or BLOCK]

[Your feedback]

See https://github.com/cloud-atlas-ai/superego for full prompt.
`;
const SUPEREGO_CONTRACT = `SUPEREGO ACTIVE: This project uses superego, a metacognitive advisor that monitors your work. When you receive SUPEREGO FEEDBACK, critically evaluate it: if you agree, incorporate it into your approach; if you disagree on non-trivial feedback, escalate to the user explaining both perspectives.`;

function loadPrompt(directory: string): string | null {
  const path = join(directory, SUPEREGO_DIR, "prompt.md");
  if (!existsSync(path)) return null;
  try {
    return readFileSync(path, "utf-8");
  } catch {
    return null;
  }
}

type SuperegoMode = "always" | "pull";

function loadMode(directory: string): SuperegoMode {
  const configPath = join(directory, SUPEREGO_DIR, "config.yaml");
  if (!existsSync(configPath)) return "pull"; // Default to pull mode
  try {
    const content = readFileSync(configPath, "utf-8");
    // Simple line-by-line parsing (no YAML dependency)
    for (const line of content.split("\n")) {
      const trimmed = line.trim();
      if (trimmed.startsWith("mode:")) {
        const value = trimmed.slice(5).trim().toLowerCase();
        if (value === "always") return "always";
        if (value === "pull") return "pull";
      }
    }
  } catch {
    // Ignore errors, default to pull
  }
  return "pull"; // Default to pull mode
}

function parseDecision(response: string): { block: boolean; feedback: string } {
  const lines = response.trim().split("\n");
  const decision = lines[0]?.trim() || "";
  const feedback = lines.slice(2).join("\n").trim();

  if (decision.startsWith("DECISION: ALLOW")) {
    return { block: false, feedback };
  }
  // Default to BLOCK for safety (including malformed responses)
  return { block: true, feedback: feedback || response };
}

function writeFeedback(directory: string, sessionId: string, feedback: string): void {
  const sessionDir = join(directory, SUPEREGO_DIR, "sessions", sessionId);
  mkdirSync(sessionDir, { recursive: true });
  writeFileSync(join(sessionDir, "feedback"), feedback);
}

// Format messages for evaluation prompt
function formatConversation(messages: any[]): string {
  return messages
    .map((m: any) => {
      const role = m.info?.role || "unknown";
      // Parts have 'text' not 'content'
      const content = m.parts?.map((p: any) => p.text || p.content || "").join("\n") || "";
      return `${role.toUpperCase()}: ${content}`;
    })
    .join("\n\n---\n\n");
}

export const Superego: Plugin = async ({ directory, client }) => {
  const superegoDir = join(directory, SUPEREGO_DIR);
  const initialized = existsSync(superegoDir);

  // Track eval sessions we create to avoid recursive evaluation
  const evalSessionIds = new Set<string>();

  if (initialized) {
    log(superegoDir, `Plugin loaded [${BUILD_VERSION}]`);
  }

  const prompt = initialized ? loadPrompt(directory) : null;
  const mode = initialized ? loadMode(directory) : "always";

  if (initialized && !prompt) {
    log(superegoDir, "No prompt.md found, evaluation disabled");
  }
  if (initialized) {
    log(superegoDir, `Mode: ${mode}`);
  }

  return {
    // Inject contract into system prompt (no LLM call needed)
    "experimental.chat.system.transform": async (_input, output) => {
      const alreadyHas = output.system.some((s: string) => s.includes("SUPEREGO ACTIVE"));
      if (initialized && !alreadyHas) {
        output.system.push(SUPEREGO_CONTRACT);
      }
    },
    tool: {
      superego: tool({
        description: "Manage superego metacognitive advisor. Commands: status (default), init, disable, enable, remove, mode.",
        args: {
          command: tool.schema.enum(["status", "init", "disable", "enable", "remove", "mode"]).default("status"),
          mode_value: tool.schema.enum(["always", "pull"]).optional().describe("Mode to set (for 'mode' command)"),
        },
        async execute({ command, mode_value }) {
          const disabledFile = join(superegoDir, ".disabled");
          const configPath = join(superegoDir, "config.yaml");

          switch (command) {
            case "status":
              if (!existsSync(superegoDir)) {
                return "Superego not initialized. Use 'superego init' to set up.";
              }
              if (existsSync(disabledFile)) {
                return "Superego initialized but DISABLED. Use 'superego enable' to re-enable.";
              }
              const hasPrompt = existsSync(join(superegoDir, "prompt.md"));
              const currentMode = loadMode(directory);
              return `Superego ENABLED. Mode: ${currentMode}. Prompt: ${hasPrompt ? "found" : "missing"}`;

            case "init":
              if (existsSync(superegoDir)) {
                return "Superego already initialized.";
              }
              mkdirSync(superegoDir, { recursive: true });
              let fetchedPrompt = FALLBACK_PROMPT;
              try {
                const response = await fetch(PROMPT_URL);
                if (response.ok) fetchedPrompt = await response.text();
              } catch {}
              writeFileSync(join(superegoDir, "prompt.md"), fetchedPrompt);
              return "Superego initialized. Restart OpenCode for hooks to take effect.";

            case "disable":
              if (!existsSync(superegoDir)) {
                return "Superego not initialized. Nothing to disable.";
              }
              if (existsSync(disabledFile)) {
                return "Superego already disabled.";
              }
              writeFileSync(disabledFile, new Date().toISOString());
              return "Superego disabled. Use 'superego enable' to re-enable.";

            case "enable":
              if (!existsSync(superegoDir)) {
                return "Superego not initialized. Use 'superego init' first.";
              }
              if (!existsSync(disabledFile)) {
                return "Superego already enabled.";
              }
              unlinkSync(disabledFile);
              return "Superego re-enabled.";

            case "remove":
              if (!existsSync(superegoDir)) {
                return "Superego not initialized. Nothing to remove.";
              }
              rmSync(superegoDir, { recursive: true, force: true });
              return "Superego removed. Restart OpenCode to complete cleanup.";

            case "mode":
              if (!existsSync(superegoDir)) {
                return "Superego not initialized. Use 'superego init' first.";
              }
              if (!mode_value) {
                // Show current mode
                const current = loadMode(directory);
                return `Current mode: ${current}. Use 'superego mode always' or 'superego mode pull' to change.`;
              }
              // Update config.yaml with new mode
              let config = "";
              if (existsSync(configPath)) {
                config = readFileSync(configPath, "utf-8");
                // Replace existing mode line or append
                if (config.includes("mode:")) {
                  config = config.replace(/^mode:.*$/m, `mode: ${mode_value}`);
                } else {
                  config = `mode: ${mode_value}\n${config}`;
                }
              } else {
                config = `mode: ${mode_value}\n`;
              }
              writeFileSync(configPath, config);
              return `Mode set to '${mode_value}'. Restart OpenCode for changes to take effect.`;
          }
        },
      }),
      superego_review: tool({
        description: "Run superego evaluation on the current session. Use at decision points: before committing to a plan, when choosing alternatives, before non-trivial implementations, when uncertain, or before claiming work is done.",
        args: {
          session_id: tool.schema.string().describe("The session ID to evaluate (required)"),
        },
        async execute({ session_id }) {
          if (!initialized) {
            return "Superego not initialized. Use 'superego init' first.";
          }
          if (!prompt) {
            return "No prompt.md found. Superego cannot evaluate.";
          }

          log(superegoDir, `Manual review requested for ${session_id}`);

          try {
            // Get conversation messages
            const messagesResult = await client.session.messages({ path: { id: session_id } });
            const messages = messagesResult.data;

            if (!messages?.length) {
              return "No messages to evaluate.";
            }

            // Extract model from session
            const originalModel = messages[0]?.info?.model;
            const modelString = originalModel ? `${originalModel.providerID}/${originalModel.modelID}` : undefined;

            // Format conversation for evaluation
            const conversation = formatConversation(messages);

            // Create eval session
            const evalSession = await client.session.create({
              body: { title: "[superego-eval]" }
            });
            const evalSessionId = (evalSession as any)?.data?.id || (evalSession as any)?.id;

            if (!evalSessionId) {
              return "Failed to create evaluation session.";
            }
            evalSessionIds.add(evalSessionId);

            const evalPrompt = `${prompt}\n\nIMPORTANT: You are a verifier only. Output DECISION and feedback text. DO NOT USE TOOLS.\n\n---\n\n## Conversation to Evaluate\n\n${conversation}`;

            log(superegoDir, `Calling LLM for review with model ${modelString || "default"}...`);
            const result = await client.session.prompt({
              path: { id: evalSessionId },
              body: {
                model: originalModel ? { providerID: originalModel.providerID, modelID: originalModel.modelID } : undefined,
                parts: [{ type: "text", text: evalPrompt }],
                tools: { write: false, edit: false, bash: false },
              },
            });

            // Extract response text
            const resultData = (result as any)?.data || result;
            const response = resultData?.parts?.map((p: any) => p.text || p.content || "").join("\n")
              || resultData?.text
              || resultData?.content
              || "";

            // Clean up eval session
            try {
              await client.session.delete({ path: { id: evalSessionId } });
            } catch {}

            const { block, feedback } = parseDecision(response);
            log(superegoDir, `Review decision: ${block ? "BLOCK" : "ALLOW"}`);

            if (block && feedback) {
              return `SUPEREGO FEEDBACK (concerns found):\n\n${feedback}`;
            } else {
              return "Superego: No concerns.";
            }
          } catch (e) {
            log(superegoDir, `ERROR: Review failed: ${e}`);
            return `Review failed: ${e}`;
          }
        },
      }),
    },
    event: async ({ event }) => {
      // Skip if not initialized or disabled
      if (!initialized) return;
      const disabledFile = join(superegoDir, ".disabled");
      if (existsSync(disabledFile)) {
        return;
      }

      // Skip automatic evaluation in pull mode (user uses review tool manually)
      if (mode === "pull") {
        return;
      }

      // Session idle - run evaluation (only in "always" mode)
      if (event.type === "session.idle") {
        const sessionId = (event as any).properties?.info?.id || (event as any).properties?.sessionID || (event as any).properties?.id;
        if (!sessionId || !prompt) {
          log(superegoDir, `session.idle skipped: sessionId=${sessionId}, hasPrompt=${!!prompt}`);
          return;
        }

        // Prevent duplicate evaluation from dual plugin instances (cost savings)
        const lockFile = join(superegoDir, `.eval-${sessionId}.lock`);
        if (existsSync(lockFile)) {
          return; // Another instance is already evaluating
        }
        writeFileSync(lockFile, Date.now().toString());

        // Skip eval sessions we created (prevent recursion)
        if (evalSessionIds.has(sessionId)) {
          log(superegoDir, `Skipping eval session ${sessionId} (in Set)`);
          evalSessionIds.delete(sessionId); // Clean up
          try { unlinkSync(lockFile); } catch {}
          return;
        }

        // Also check session title for eval marker (handles dual plugin instance issue)
        try {
          const sessionInfo = await client.session.get({ path: { id: sessionId } });
          const title = (sessionInfo as any)?.data?.title || (sessionInfo as any)?.title || "";
          if (title.includes("[superego-eval]")) {
            log(superegoDir, `Skipping eval session ${sessionId} (by title)`);
            try { unlinkSync(lockFile); } catch {}
            return;
          }
        } catch {
          // If we can't get session info, proceed with evaluation
        }

        log(superegoDir, `Evaluating ${sessionId}...`);

        try {
          // Get conversation messages
          const messagesResult = await client.session.messages({ path: { id: sessionId } });
          const messages = messagesResult.data;
          log(superegoDir, `Got ${messages?.length || 0} messages`);
          if (messages?.length) {
            log(superegoDir, `First message structure: ${JSON.stringify(messages[0], null, 2)}`);
          }

          if (!messages?.length) {
            log(superegoDir, "No messages to evaluate");
            return;
          }

          // Extract model from original session to use for eval
          const originalModel = messages[0]?.info?.model;
          const modelString = originalModel ? `${originalModel.providerID}/${originalModel.modelID}` : undefined;
          log(superegoDir, `Original session model: ${modelString || "unknown"}`);

          // Format conversation for evaluation
          const conversation = formatConversation(messages);

          // Test mode: magic phrase triggers instant BLOCK without LLM call
          if (conversation.includes("[SUPEREGO-TEST-BLOCK]")) {
            log(superegoDir, "Test mode: triggering BLOCK");
            const testFeedback = "This is a test BLOCK triggered by [SUPEREGO-TEST-BLOCK]. The superego feedback injection is working correctly.";
            writeFeedback(directory, sessionId, testFeedback);
            try {
              await client.session.prompt({
                path: { id: sessionId },
                body: {
                  parts: [{ type: "text", text: `SUPEREGO FEEDBACK:\n\n${testFeedback}` }],
                },
              });
              log(superegoDir, "Test feedback injected");
            } catch (e) {
              log(superegoDir, `ERROR: Failed to inject test feedback: ${e}`);
            }
            return;
          }

          // Create eval session and get response via OpenCode's configured LLM
          log(superegoDir, "Creating eval session...");
          // Mark eval sessions with distinctive title so we can skip them
          const evalSession = await client.session.create({
            body: { title: "[superego-eval]" }
          });
          // Response structure: { data: { id: "ses_..." }, request: {}, response: {} }
          const evalSessionId = (evalSession as any)?.data?.id || (evalSession as any)?.id;

          if (!evalSessionId) {
            log(superegoDir, `ERROR: Failed to create eval session. Response: ${JSON.stringify(evalSession)}`);
            return;
          }
          log(superegoDir, `Eval session created: ${evalSessionId}`);
          evalSessionIds.add(evalSessionId); // Track to prevent recursive evaluation

          const evalPrompt = `${prompt}\n\nIMPORTANT: You are a verifier only. Output DECISION and feedback text. DO NOT USE TOOLS.\n\n---\n\n## Conversation to Evaluate\n\n${conversation}`;

          log(superegoDir, `Calling LLM via OpenCode with model ${modelString || "default"}...`);
          // session.prompt() returns the AssistantMessage response directly
          // Pass model explicitly to use same model as original session
          const result = await client.session.prompt({
            path: { id: evalSessionId },
            body: {
              model: originalModel ? { providerID: originalModel.providerID, modelID: originalModel.modelID } : undefined,
              parts: [{ type: "text", text: evalPrompt }],
              tools: { write: false, edit: false, bash: false },  // Eval session has no tools
            },
          });

          // Extract response text
          log(superegoDir, `Raw result keys: ${Object.keys(result || {}).join(", ")}`);
          log(superegoDir, `Raw result: ${JSON.stringify(result).slice(0, 500)}`);

          // Try multiple paths for response extraction
          const resultData = (result as any)?.data || result;
          const response = resultData?.parts?.map((p: any) => p.text || p.content || "").join("\n")
            || resultData?.text
            || resultData?.content
            || "";
          log(superegoDir, `LLM response: ${response.slice(0, 200)}`);

          // Clean up eval session
          try {
            await client.session.delete({ path: { id: evalSessionId } });
          } catch {
            // Ignore cleanup errors
          }

          const { block, feedback } = parseDecision(response);
          log(superegoDir, `Decision: ${block ? "BLOCK" : "ALLOW"}`);

          if (block && feedback) {
            writeFeedback(directory, sessionId, feedback);
            log(superegoDir, `Feedback written to .superego/sessions/${sessionId}/feedback`);

            // Inject feedback into the original session so model sees it
            try {
              await client.session.prompt({
                path: { id: sessionId },
                body: {
                  parts: [{ type: "text", text: `SUPEREGO FEEDBACK:\n\n${feedback}` }],
                },
              });
              log(superegoDir, `Feedback injected into session ${sessionId}`);
            } catch (e) {
              log(superegoDir, `ERROR: Failed to inject feedback: ${e}`);
            }
          }
        } catch (e) {
          log(superegoDir, `ERROR: Evaluation failed: ${e}`);
        } finally {
          // Clean up lock file
          try { unlinkSync(lockFile); } catch {}
        }
      }
    },
  };
};

export default Superego;
