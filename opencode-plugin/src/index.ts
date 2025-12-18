/**
 * Superego OpenCode Plugin
 *
 * Metacognitive advisor for OpenCode. Injects contract on session start,
 * evaluates via Gemini on session idle.
 */

import type { Plugin } from "@opencode-ai/plugin";
import { existsSync, readFileSync, mkdirSync, writeFileSync } from "fs";
import { join } from "path";
import { GoogleGenerativeAI } from "@google/generative-ai";

const SUPEREGO_DIR = ".superego";
const SUPEREGO_CONTRACT = `SUPEREGO ACTIVE: This project uses superego, a metacognitive advisor that monitors your work. When you receive SUPEREGO FEEDBACK, critically evaluate it: if you agree, incorporate it into your approach; if you disagree on non-trivial feedback, escalate to the user explaining both perspectives.`;
const DEFAULT_MODEL = "gemini-2.5-pro";

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

export const Superego: Plugin = async ({ directory, client }) => {
  const superegoDir = join(directory, SUPEREGO_DIR);

  // Skip if not initialized
  if (!existsSync(superegoDir)) {
    console.log("[superego] Not initialized, skipping");
    return {};
  }

  console.log("[superego] Plugin loaded");

  const prompt = loadPrompt(directory);
  if (!prompt) {
    console.log("[superego] No prompt.md found, evaluation disabled");
  }

  const apiKey = process.env.GOOGLE_API_KEY || process.env.GEMINI_API_KEY;
  if (!apiKey) {
    console.log("[superego] No GOOGLE_API_KEY set, evaluation disabled");
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
            await client.session.prompt({
              body: { sessionID: sessionId, content: SUPEREGO_CONTRACT },
              query: { assistant: false },
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
        if (!sessionId || !prompt || !apiKey) return;

        console.log(`[superego] Session idle: ${sessionId}, evaluating...`);

        try {
          // NEEDS VALIDATION: What does client.session.messages() actually return?
          const messages = await client.session.messages({ path: { id: sessionId } });
          console.log(`[superego] Got ${messages?.length || 0} messages`);
          if (messages?.length) {
            console.log("[superego] First message structure:", JSON.stringify(messages[0], null, 2));
          }

          if (!messages?.length) {
            console.log("[superego] No messages to evaluate");
            return;
          }

          // NEEDS VALIDATION: Is this the right structure for messages?
          const conversation = messages
            .map((m: any) => {
              const role = m.info?.role || "unknown";
              const content = m.parts?.map((p: any) => p.content || "").join("\n") || "";
              return `${role.toUpperCase()}: ${content}`;
            })
            .join("\n\n---\n\n");

          // Call Gemini
          const genAI = new GoogleGenerativeAI(apiKey);
          const model = genAI.getGenerativeModel({ model: DEFAULT_MODEL });
          const evalPrompt = `${prompt}\n\n---\n\n## Conversation to Evaluate\n\n${conversation}`;

          console.log("[superego] Calling Gemini...");
          const result = await model.generateContent(evalPrompt);
          const response = result.response.text();
          console.log("[superego] Gemini response:", response.slice(0, 200));

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
