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
// NEEDS VALIDATION: Message structure assumed based on OpenCode SDK docs
function formatConversation(messages: any[]): string {
  return messages
    .map((m: any) => {
      const role = m.info?.role || "unknown";
      const content = m.parts?.map((p: any) => p.content || "").join("\n") || "";
      return `${role.toUpperCase()}: ${content}`;
    })
    .join("\n\n---\n\n");
}

export const Superego: Plugin = async ({ directory, client }) => {
  const superegoDir = join(directory, SUPEREGO_DIR);
  const initialized = existsSync(superegoDir);

  if (initialized) {
    log(superegoDir, "Plugin loaded");
  }

  const prompt = initialized ? loadPrompt(directory) : null;
  if (initialized && !prompt) {
    log(superegoDir, "No prompt.md found, evaluation disabled");
  }

  return {
    tool: {
      superego: tool({
        description: "Manage superego metacognitive advisor. Commands: status (default), init, disable, enable, remove.",
        args: {
          command: tool.schema.enum(["status", "init", "disable", "enable", "remove"]).default("status"),
        },
        async execute({ command }) {
          const disabledFile = join(superegoDir, ".disabled");

          switch (command) {
            case "status":
              if (!existsSync(superegoDir)) {
                return "Superego not initialized. Use 'superego init' to set up.";
              }
              if (existsSync(disabledFile)) {
                return "Superego initialized but DISABLED. Use 'superego enable' to re-enable.";
              }
              const hasPrompt = existsSync(join(superegoDir, "prompt.md"));
              return `Superego ENABLED. Prompt: ${hasPrompt ? "found" : "missing"}`;

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

      // Session created - inject contract
      if (event.type === "session.created") {
        const sessionId = (event as any).properties?.info?.id;
        log(superegoDir, `Session created: ${sessionId}`);

        if (sessionId) {
          try {
            // Inject contract without triggering response (noReply: true)
            await client.session.prompt({
              path: { id: sessionId },
              body: {
                noReply: true,
                parts: [{ type: "text", text: SUPEREGO_CONTRACT }],
              },
            });
            log(superegoDir, "Contract injected");
          } catch (e) {
            log(superegoDir, `ERROR: Failed to inject contract: ${e}`);
          }
        }
      }

      // Session idle - run evaluation
      if (event.type === "session.idle") {
        const sessionId = (event as any).properties?.info?.id;
        if (!sessionId || !prompt) return;

        log(superegoDir, `Session idle: ${sessionId}, evaluating...`);

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

          // Format conversation for evaluation
          const conversation = formatConversation(messages);

          // Create eval session and get response via OpenCode's configured LLM
          log(superegoDir, "Creating eval session...");
          const evalSession = await client.session.create({ body: {} });
          const evalSessionId = (evalSession as any)?.id;

          if (!evalSessionId) {
            log(superegoDir, "ERROR: Failed to create eval session");
            return;
          }

          const evalPrompt = `${prompt}\n\n---\n\n## Conversation to Evaluate\n\n${conversation}`;

          log(superegoDir, "Calling LLM via OpenCode...");
          // session.prompt() returns the AssistantMessage response directly
          const result = await client.session.prompt({
            path: { id: evalSessionId },
            body: {
              parts: [{ type: "text", text: evalPrompt }],
            },
          });

          // Extract response text
          // NEEDS VALIDATION: What's the actual response structure?
          const response = (result as any)?.parts?.map((p: any) => p.text || p.content || "").join("\n") || "";
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
          }
        } catch (e) {
          log(superegoDir, `ERROR: Evaluation failed: ${e}`);
        }
      }
    },
  };
};

export default Superego;
