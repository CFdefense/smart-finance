import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { Observable } from 'rxjs';

import { API_BASE_URL } from '../api-config';

import type {
  ChatSession,
  ChatSessionWithMessages,
  ChatSessionsResponse,
  ChatStreamEvent,
} from '../models/chat.models';

/**
 * Connects the Angular chatbot to the authenticated Rust API.
 *
 * Regular requests use HttpClient. Streaming uses fetch because
 * we need access to response chunks before the reply finishes.
 */
@Injectable({ providedIn: 'root' })
export class ChatService {
  private readonly http = inject(HttpClient);
  private readonly baseUrl = `${API_BASE_URL}/chat`;
  private readonly options = { withCredentials: true };

  /** List the signed-in user's conversations, newest first. */
  getSessions(): Observable<ChatSessionsResponse> {
    return this.http.get<ChatSessionsResponse>(`${this.baseUrl}/sessions`, this.options);
  }

  /** Create a new, initially empty conversation. */
  createSession(): Observable<ChatSession> {
    return this.http.post<ChatSession>(`${this.baseUrl}/sessions`, {}, this.options);
  }

  /** Retrieve one conversation and all its saved messages. */
  getSession(id: number): Observable<ChatSessionWithMessages> {
    return this.http.get<ChatSessionWithMessages>(`${this.baseUrl}/sessions/${id}`, this.options);
  }

  /**
   * Permanently delete a conversation.
   *
   * The matching Rust DELETE route will be added in Phase 4.
   * PostgreSQL already supports cascading message deletion.
   */
  deleteSession(id: number): Observable<void> {
    return this.http.delete<void>(`${this.baseUrl}/sessions/${id}`, this.options);
  }

  /**
   * Send a message and process the streamed assistant response.
   *
   * The callback receives events as they arrive, allowing the
   * component to update its status and text immediately.
   *
   * AbortSignal lets the component cancel an active request
   * when it is destroyed or the conversation changes.
   *
   * Resolves only after receiving a successful "done" event.
   * Throws if the request fails or ends without completion.
   */
  async sendMessage(
    sessionId: number,
    message: string,
    onEvent: (event: ChatStreamEvent) => void,
    signal?: AbortSignal,
  ): Promise<void> {
    const response = await fetch(`${this.baseUrl}/sessions/${sessionId}/messages`, {
      method: 'POST',
      credentials: 'include',
      headers: {
        'Content-Type': 'application/json',
        Accept: 'text/event-stream',
      },
      body: JSON.stringify({ message }),
      signal,
    });

    if (!response.ok) {
      if (response.status === 409) {
        throw new Error('A response is already being generated.');
      }

      if (response.status === 503) {
        throw new Error('The AI assistant is currently unavailable.');
      }

      throw new Error(`Chat request failed (HTTP ${response.status}).`);
    }

    if (!response.body) {
      throw new Error('The server did not provide a response stream.');
    }

    const reader = response.body.getReader();
    const decoder = new TextDecoder();

    // A single network chunk might contain only part of an SSE line.
    let lineBuffer = '';
    let eventName = '';
    let eventData: string[] = [];
    let completed = false;

    /**
     * Process one complete SSE event block.
     *
     * Rust sends an event name and JSON data. Keepalive comments
     * have no event name and are safely ignored.
     */
    const dispatchEvent = (): void => {
      if (!eventName || eventData.length === 0) {
        eventName = '';
        eventData = [];
        return;
      }

      const event = {
        type: eventName,
        ...JSON.parse(eventData.join('\n')),
      } as ChatStreamEvent;

      eventName = '';
      eventData = [];

      onEvent(event);

      if (event.type === 'done') {
        completed = true;
      } else if (event.type === 'error') {
        throw new Error(event.message);
      }
    };

    /**
     * SSE events are separated by blank lines. Process complete
     * lines while retaining any unfinished line for the next chunk.
     */
    const processText = (text: string): void => {
      lineBuffer += text;

      let newlineIndex: number;

      while ((newlineIndex = lineBuffer.indexOf('\n')) !== -1) {
        const line = lineBuffer.slice(0, newlineIndex).replace(/\r$/, '');
        lineBuffer = lineBuffer.slice(newlineIndex + 1);

        if (line === '') {
          dispatchEvent();
        } else if (line.startsWith('event:')) {
          eventName = line.slice(6).trim();
        } else if (line.startsWith('data:')) {
          eventData.push(line.slice(5).trimStart());
        }
        // SSE comments and fields we do not need are ignored.
      }
    };

    try {
      while (!completed) {
        const { value, done } = await reader.read();

        if (done) {
          break;
        }

        processText(decoder.decode(value, { stream: true }));
      }

      // Flush remaining decoded text if the connection ended.
      if (!completed) {
        processText(decoder.decode());

        if (lineBuffer) {
          processText('\n');
        }

        dispatchEvent();
      }

      if (!completed) {
        throw new Error('The response ended before it was saved.');
      }
    } finally {
      // Release the stream even on an error or cancellation.
      await reader.cancel().catch(() => undefined);
      reader.releaseLock();
    }
  }
}
