/**
 * Superego OpenCode Plugin - Minimal viable version
 *
 * Phase 1: Just inject the contract on session start.
 * Validates the hook system works before adding complexity.
 */

import type { Plugin } from "@opencode-ai/plugin";
import { existsSync } from "fs";
import { join } from "path";

const SUPEREGO_DIR = ".superego";
const SUPEREGO_CONTRACT = `SUPEREGO ACTIVE: This project uses superego, a metacognitive advisor that monitors your work. When you receive SUPEREGO FEEDBACK, critically evaluate it: if you agree, incorporate it into your approach; if you disagree on non-trivial feedback, escalate to the user explaining both perspectives.`;

export const Superego: Plugin = async ({ directory, client }) => {
  const superegoDir = join(directory, SUPEREGO_DIR);

  // Skip if not initialized
  if (!existsSync(superegoDir)) {
    console.log("[superego] Not initialized, skipping");
    return {};
  }

  console.log("[superego] Plugin loaded");

  return {
    event: async ({ event }) => {
      if (event.type === "session.created") {
        const sessionId = (event as any).properties?.id;
        console.log(`[superego] Session created: ${sessionId}`);

        if (sessionId) {
          try {
            // Attempt to inject contract - API needs validation
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
    },
  };
};

export default Superego;
