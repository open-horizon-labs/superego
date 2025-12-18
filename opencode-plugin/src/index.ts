/**
 * Superego OpenCode Plugin
 *
 * Metacognitive advisor for OpenCode. Injects contract on session start,
 * evaluates via OpenCode's configured LLM on session idle.
 */

import type { Plugin } from "@opencode-ai/plugin";
import { existsSync, readFileSync, mkdirSync, writeFileSync } from "fs";
import { join } from "path";

const SUPEREGO_DIR = ".superego";
const SUPEREGO_CONTRACT = `SUPEREGO ACTIVE: This project uses superego, a metacognitive advisor that monitors your work. When you receive SUPEREGO FEEDBACK, critically evaluate it: if you agree, incorporate it into your approach; if you disagree on non-trivial feedback, escalate to the user explaining both perspectives.`;

const PROMPT_URL = "https://raw.githubusercontent.com/cloud-atlas-ai/superego/main/default_prompt.md";

// Fallback prompt if fetch fails
const FALLBACK_PROMPT = `# Superego System Prompt

You are **Superego**, a metacognitive advisor. Respond with:

DECISION: [ALLOW or BLOCK]

[Your feedback]

See https://github.com/cloud-atlas-ai/superego for full prompt.
`;

async function initSuperego(directory: string): Promise<void> {
  const superegoDir = join(directory, SUPEREGO_DIR);
  mkdirSync(superegoDir, { recursive: true });

  // Fetch full prompt from GitHub
  let prompt = FALLBACK_PROMPT;
  try {
    const response = await fetch(PROMPT_URL);
    if (response.ok) {
      prompt = await response.text();
      console.log("[superego] Fetched full prompt from GitHub");
    }
  } catch (e) {
    console.log("[superego] Could not fetch prompt, using fallback");
  }

  writeFileSync(join(superegoDir, "prompt.md"), prompt);
  console.log(`[superego] Initialized ${superegoDir}/prompt.md`);
}

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

  // Auto-initialize if not present
  if (!existsSync(superegoDir)) {
    await initSuperego(directory);
    console.log("[superego] ⚠️  First run - please restart OpenCode for hooks to take effect");
    console.log("[superego] Edit .superego/prompt.md to customize evaluation criteria");
    return {};
  }

  console.log("[superego] Plugin loaded");

  const prompt = loadPrompt(directory);
  if (!prompt) {
    console.log("[superego] No prompt.md found, evaluation disabled");
  }

  return {
    event: async ({ event }) => {
      // Session created - inject contract
      // NEEDS VALIDATION: Does session.created fire? Is properties.id correct?
      if (event.type === "session.created") {
        const sessionId = (event as any).properties?.id;
        console.log(`[superego] Session created: ${sessionId}`);

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
            console.log("[superego] Contract injected");
          } catch (e) {
            console.error("[superego] Failed to inject contract:", e);
          }
        }
      }

      // Session idle - run evaluation
      // NEEDS VALIDATION: Does session.idle fire? What's the actual message structure?
      if (event.type === "session.idle") {
        const sessionId = (event as any).properties?.id;
        if (!sessionId || !prompt) return;

        console.log(`[superego] Session idle: ${sessionId}, evaluating...`);

        try {
          // Get conversation messages
          const messages = await client.session.messages({ path: { id: sessionId } });
          console.log(`[superego] Got ${messages?.length || 0} messages`);
          if (messages?.length) {
            console.log("[superego] First message structure:", JSON.stringify(messages[0], null, 2));
          }

          if (!messages?.length) {
            console.log("[superego] No messages to evaluate");
            return;
          }

          // Format conversation for evaluation
          const conversation = formatConversation(messages);

          // Create eval session and get response via OpenCode's configured LLM
          console.log("[superego] Creating eval session...");
          const evalSession = await client.session.create({ body: { directory } });
          const evalSessionId = (evalSession as any)?.id;

          if (!evalSessionId) {
            console.error("[superego] Failed to create eval session");
            return;
          }

          const evalPrompt = `${prompt}\n\n---\n\n## Conversation to Evaluate\n\n${conversation}`;

          console.log("[superego] Calling LLM via OpenCode...");
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
          console.log("[superego] LLM response:", response.slice(0, 200));

          // Clean up eval session
          try {
            await client.session.delete({ path: { id: evalSessionId } });
          } catch {
            // Ignore cleanup errors
          }

          const { block, feedback } = parseDecision(response);
          console.log(`[superego] Decision: ${block ? "BLOCK" : "ALLOW"}`);

          if (block && feedback) {
            writeFeedback(directory, sessionId, feedback);
            console.log(`[superego] Feedback written to .superego/sessions/${sessionId}/feedback`);
            // TODO: Find way to surface feedback to user in OpenCode UI
          }
        } catch (e) {
          console.error("[superego] Evaluation failed:", e);
        }
      }
    },
  };
};

export default Superego;
