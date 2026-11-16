import { Component, inject, OnDestroy, OnInit, signal } from '@angular/core';
import { firstValueFrom } from 'rxjs';

import { ChatService } from '../services/chat.service';
import type { ChatMessage, ChatSession, ChatStreamEvent } from '../models/chat.models';

/**
 * Floating AI assistant for the authenticated dashboard.
 *
 * Saved conversations live in PostgreSQL. Only temporary UI
 * state (draft text, loading indicators, streamed text) is
 * maintained here.
 */
@Component({
  selector: 'app-chat',
  standalone: true,
  templateUrl: './chat.html',
  styleUrl: './chat.css',
})
export class ChatWidget implements OnInit, OnDestroy {
  private readonly chatService = inject(ChatService);

  // Window and conversation navigation.
  readonly expanded = signal(false);
  readonly showingHistory = signal(false);
  readonly sessions = signal<ChatSession[]>([]);
  readonly activeSessionId = signal<number | null>(null);
  readonly messages = signal<ChatMessage[]>([]);

  // Composer and live response state.
  readonly draft = signal('');
  readonly pendingMessage = signal<string | null>(null);
  readonly streamedText = signal('');
  readonly activity = signal<string[]>([]);
  readonly currentStatus = signal<string | null>(null);

  // Request and error states.
  readonly sending = signal(false);
  readonly loadingSessions = signal(false);
  readonly loadingConversation = signal(false);
  readonly deletingSessionId = signal<number | null>(null);
  readonly confirmDeleteId = signal<number | null>(null);
  readonly errorMessage = signal<string | null>(null);
  readonly historyError = signal<string | null>(null);

  private streamController?: AbortController;
  private destroyed = false;

  ngOnInit(): void {
    void this.loadSessions();
  }

  ngOnDestroy(): void {
    this.destroyed = true;
    this.streamController?.abort();
  }

  /** Expand or minimize without losing the current conversation. */
  toggleWindow(): void {
    this.expanded.update((expanded) => !expanded);
  }

  /** Show or hide the conversation-history panel. */
  toggleHistory(): void {
    this.showingHistory.update((showing) => !showing);
    this.confirmDeleteId.set(null);

    if (this.showingHistory()) {
      void this.loadSessions();
    }
  }

  /** Retrieve the user's saved conversations. */
  async loadSessions(): Promise<void> {
    this.loadingSessions.set(true);
    this.historyError.set(null);

    try {
      const result = await firstValueFrom(this.chatService.getSessions());

      if (!this.destroyed) {
        this.sessions.set(result.sessions);
      }
    } catch {
      if (!this.destroyed) {
        this.historyError.set('Unable to load conversation history.');
      }
    } finally {
      if (!this.destroyed) {
        this.loadingSessions.set(false);
      }
    }
  }

  /**
   * Start composing a new conversation.
   *
   * We create its database row only when the first message
   * is sent, avoiding empty chats from accidental clicks.
   */
  newChat(): void {
    if (this.sending() || this.loadingConversation()) {
      return;
    }

    this.activeSessionId.set(null);
    this.messages.set([]);
    this.draft.set('');
    this.clearResponse();
    this.showingHistory.set(false);
    this.confirmDeleteId.set(null);
  }

  /** Open an existing conversation with its saved messages. */
  async openSession(id: number): Promise<void> {
    if (this.sending() || this.loadingConversation()) {
      return;
    }

    this.loadingConversation.set(true);
    this.errorMessage.set(null);

    try {
      const session = await firstValueFrom(this.chatService.getSession(id));

      if (this.destroyed) {
        return;
      }

      this.activeSessionId.set(session.id);
      this.messages.set(session.messages);
      this.draft.set('');
      this.clearResponse();
      this.showingHistory.set(false);
    } catch {
      this.errorMessage.set('Unable to open this conversation.');
      this.showingHistory.set(false);
    } finally {
      this.loadingConversation.set(false);
    }
  }

  /** Ask for confirmation before permanently deleting a chat. */
  requestDelete(id: number): void {
    if (this.sending()) {
      return;
    }

    this.confirmDeleteId.set(id);
  }

  /**
   * Delete a session using the authenticated Rust endpoint.
   * If the active conversation is removed, return to a new chat.
   */
  async deleteSession(id: number): Promise<void> {
    if (this.sending() || this.deletingSessionId() !== null) {
      return;
    }

    this.deletingSessionId.set(id);
    this.historyError.set(null);

    try {
      await firstValueFrom(this.chatService.deleteSession(id));

      this.sessions.update((sessions) => sessions.filter((session) => session.id !== id));

      if (this.activeSessionId() === id) {
        this.activeSessionId.set(null);
        this.messages.set([]);
        this.draft.set('');
        this.clearResponse();
      }

      this.confirmDeleteId.set(null);
    } catch {
      this.historyError.set('Unable to delete this conversation.');
    } finally {
      this.deletingSessionId.set(null);
    }
  }

  /** Enter sends a message; Shift+Enter inserts a new line. */
  onComposerKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      void this.send();
    }
  }

  /** Convert backend tool names into readable activity labels. */
  private describeTool(tool: string): string {
    switch (tool) {
      case 'web_search':
        return 'Searching the web...';
      case 'get_transactions':
        return 'Checking transactions...';
      default:
        return `Using ${tool.replace(/_/g, ' ')}...`;
    }
  }

  /**
   * Apply each real event from the AI stream.
   *
   * Tool events update activity; token events append generated
   * text. "done" signals that Rust saved the completed reply.
   */
  private handleStreamEvent(event: ChatStreamEvent): void {
    switch (event.type) {
      case 'status': {
        const description = this.describeTool(event.tool);
        this.currentStatus.set(description);

        if (!this.activity().includes(description)) {
          this.activity.update((items) => [...items, description]);
        }
        break;
      }

      case 'token':
        this.streamedText.update((text) => text + event.text);
        this.currentStatus.set(null);
        break;

      case 'done':
        this.currentStatus.set(null);
        break;

      case 'error':
        this.currentStatus.set(null);
        break;
    }
  }

  /**
   * Send the draft and stream the response into the interface.
   *
   * A user message stays temporary until the backend confirms
   * completion. We then reload the authoritative saved messages.
   */
  async send(): Promise<void> {
    const message = this.draft().trim();

    if (!message || message.length > 2000 || this.sending() || this.loadingConversation()) {
      return;
    }

    this.sending.set(true);
    this.clearResponse();

    try {
      let sessionId = this.activeSessionId();

      if (sessionId === null) {
        const session = await firstValueFrom(this.chatService.createSession());

        if (this.destroyed) {
          return;
        }

        sessionId = session.id;
        this.activeSessionId.set(sessionId);
      }

      this.pendingMessage.set(message);
      this.draft.set('');
      this.currentStatus.set('Preparing your answer...');

      const controller = new AbortController();
      this.streamController = controller;

      await this.chatService.sendMessage(
        sessionId,
        message,
        (event) => this.handleStreamEvent(event),
        controller.signal,
      );

      if (this.destroyed) {
        return;
      }

      // Refresh from PostgreSQL after the successful save.
      const saved = await firstValueFrom(this.chatService.getSession(sessionId));

      this.messages.set(saved.messages);
      this.pendingMessage.set(null);
      this.streamedText.set('');
      this.currentStatus.set(null);

      await this.loadSessions();
    } catch (error) {
      if (this.destroyed) {
        return;
      }

      this.errorMessage.set(
        error instanceof Error ? error.message : 'The assistant could not complete your request.',
      );

      // Restore the unsaved draft so the user can try again.
      if (!this.draft()) {
        this.draft.set(message);
      }

      this.pendingMessage.set(null);
      this.streamedText.set('');
      this.currentStatus.set(null);
    } finally {
      this.streamController = undefined;

      if (!this.destroyed) {
        this.sending.set(false);
      }
    }
  }

  /** Remove transient response state, preserving saved messages. */
  private clearResponse(): void {
    this.pendingMessage.set(null);
    this.streamedText.set('');
    this.activity.set([]);
    this.currentStatus.set(null);
    this.errorMessage.set(null);
  }
}
