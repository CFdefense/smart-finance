/**
 * Shared TypeScript models for the AI financial assistant.
 *
 * These interfaces mirror the Rust chat HTTP models so
 * components can safely work with sessions and messages.
 */

export interface ChatMessage {
  id: number;
  role: 'user' | 'assistant';
  content: string;
  created_at: string;
}

export interface ChatSession {
  id: number;
  title: string | null;
  created_at: string;
  updated_at: string;
}

export interface ChatSessionsResponse {
  sessions: ChatSession[];
}

export interface ChatSessionWithMessages extends ChatSession {
  messages: ChatMessage[];
}

/**
 * Events sent by the Rust backend through Server-Sent Events.
 *
 * status: An AI tool is being used.
 * token: A piece of the generated assistant response.
 * done: The completed response was saved successfully.
 * error: The AI request or database save failed.
 */
export type ChatStreamEvent =
  | { type: 'status'; tool: string }
  | { type: 'token'; text: string }
  | { type: 'done'; message: ChatMessage }
  | { type: 'error'; code: string; message: string };
