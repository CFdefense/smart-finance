import {
  afterNextRender,
  Component,
  ElementRef,
  inject,
  Injector,
  OnDestroy,
  OnInit,
  signal,
  ViewChild,
} from '@angular/core';

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

  private readonly injector = inject(Injector);

  @ViewChild('messageList')
  private messageList?: ElementRef<HTMLDivElement>;

  /**
   * Follow incoming text unless the user deliberately scrolls up.
   *
   * Reading the scroll position lets us distinguish someone browsing
   * an earlier answer from someone waiting for the latest response.
   */
  private followOutput = true;

  /** Whether the previously submitted message failed to send. */
  readonly failedMessage = signal<string | null>(null);

  /**
   * Update our follow preference when the user scrolls.
   * A small threshold avoids floating-point layout differences.
   */
  onMessagesScroll(): void {
    const element = this.messageList?.nativeElement;

    if (!element) {
      return;
    }

    const distanceFromBottom = element.scrollHeight - element.scrollTop - element.clientHeight;

    this.followOutput = distanceFromBottom <= 80;
  }

  /**
   * Scroll after Angular has rendered the updated messages.
   *
   * Scheduling after rendering is important: scrolling immediately
   * after updating a signal may use the old content height.
   */
  private scheduleScroll(force = false): void {
    if (!force && !this.followOutput) {
      return;
    }

    afterNextRender(
      () => {
        if (!this.expanded() || this.showingHistory()) {
          return;
        }

        const element = this.messageList?.nativeElement;

        if (element && (force || this.followOutput)) {
          element.scrollTop = element.scrollHeight;
        }
      },
      { injector: this.injector },
    );
  }

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
    const opening = !this.expanded();

    this.expanded.set(opening);

    if (opening) {
      this.followOutput = true;
      this.scheduleScroll(true);
    }
  }

  /** Show or hide the conversation-history panel. */
  toggleHistory(): void {
    this.showingHistory.update((showing) => !showing);
    this.confirmDeleteId.set(null);

    if (this.showingHistory()) {
      void this.loadSessions();
    }

    if (!this.showingHistory()) {
      this.followOutput = true;
      this.scheduleScroll(true);
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

      // Jump to the newest message after opening a saved conversation.
      this.followOutput = true;
      this.scheduleScroll(true);
    } catch {
      if (!this.destroyed) {
        this.errorMessage.set('Unable to open this conversation.');
        this.showingHistory.set(false);
      }
    } finally {
      // Always re-enable the composer when the request finishes.
      if (!this.destroyed) {
        this.loadingConversation.set(false);
      }
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
   * Tool statuses come from the backend. We do not invent
   * reasoning steps or display the model's private reasoning.
   */
  private handleStreamEvent(event: ChatStreamEvent): void {
    switch (event.type) {
      case 'status': {
        const description = this.describeTool(event.tool);

        this.currentStatus.set(description);

        if (!this.activity().includes(description)) {
          this.activity.update((items) => [...items, description]);
        }

        this.scheduleScroll();
        break;
      }

      case 'token':
        this.streamedText.update((text) => text + event.text);
        this.currentStatus.set(null);
        this.scheduleScroll();
        break;

      case 'done':
        this.currentStatus.set('Saving conversation...');
        this.scheduleScroll();
        break;

      case 'error':
        this.currentStatus.set(null);
        break;
    }
  }

  /**
   * Send the current draft, stream the answer, and reconcile
   * the interface with the saved PostgreSQL conversation.
   *
   * An AI failure is different from a history-refresh failure:
   * only an AI failure should offer the original question for retry.
   */
  async send(): Promise<void> {
    const message = this.draft().trim();

    if (!message || message.length > 2000 || this.sending() || this.loadingConversation()) {
      return;
    }

    this.sending.set(true);
    this.clearResponse();

    // The "done" event contains the assistant message saved by Rust.
    const completion: { reply: ChatMessage | null } = { reply: null };

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

      // Display the submitted question while the AI is working.
      this.pendingMessage.set(message);
      this.draft.set('');
      this.currentStatus.set('Preparing your answer...');

      this.followOutput = true;
      this.scheduleScroll(true);

      const controller = new AbortController();
      this.streamController = controller;

      await this.chatService.sendMessage(
        sessionId,
        message,
        (event) => {
          this.handleStreamEvent(event);

          if (event.type === 'done') {
            completion.reply = event.message;
          }
        },
        controller.signal,
      );

      if (this.destroyed) {
        return;
      }

      /*
       * A successful stream means Rust has committed the turn.
       * Fetch the authoritative session to obtain both saved
       * message IDs and the correct chronological history.
       */
      try {
        const saved = await firstValueFrom(this.chatService.getSession(sessionId));

        if (this.destroyed) {
          return;
        }

        this.messages.set(saved.messages);
      } catch {
        /*
         * The answer was saved, even though reloading failed.
         * Preserve the visible conversation using the confirmed
         * reply instead of incorrectly inviting another send.
         *
         * The negative ID identifies a temporary display-only
         * message. Reopening the conversation restores real IDs.
         */
        if (completion.reply !== null) {
          const reply: ChatMessage = completion.reply;

          this.messages.update((existing) => [
            ...existing,
            {
              id: -Date.now(),
              role: 'user',
              content: message,
              created_at: new Date().toISOString(),
            },
            reply,
          ]);
        }

        this.errorMessage.set(
          'Your answer was saved, but history could not refresh. ' +
            'Reopen this conversation to synchronize it.',
        );
      }

      this.pendingMessage.set(null);
      this.streamedText.set('');
      this.currentStatus.set(null);
      this.activity.set([]);

      this.scheduleScroll();
      await this.loadSessions();
    } catch (error) {
      if (this.destroyed) {
        return;
      }

      /*
       * An AI or network failure before "done" does not save
       * the new turn. Preserve the user's question for retry.
       */
      this.errorMessage.set(
        error instanceof Error ? error.message : 'The assistant could not complete your request.',
      );

      this.failedMessage.set(message);

      if (!this.draft()) {
        this.draft.set(message);
      }

      this.pendingMessage.set(null);
      this.streamedText.set('');
      this.currentStatus.set(null);
      this.activity.set([]);

      this.scheduleScroll();
    } finally {
      this.streamController = undefined;

      if (!this.destroyed) {
        this.sending.set(false);
      }
    }
  }

  /**
   * Clear transient response state.
   *
   * This never changes messages loaded from PostgreSQL.
   */
  private clearResponse(): void {
    this.pendingMessage.set(null);
    this.failedMessage.set(null);
    this.streamedText.set('');
    this.activity.set([]);
    this.currentStatus.set(null);
    this.errorMessage.set(null);
  }
}
