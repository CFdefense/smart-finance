import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { of, throwError } from 'rxjs';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ChatWidget } from './chat';
import { ChatService } from '../services/chat.service';
import type {
  ChatMessage,
  ChatSession,
  ChatSessionWithMessages,
  ChatStreamEvent,
} from '../models/chat.models';

/**
 * Tests the floating chatbot as an Angular component.
 *
 * All service methods are mocked. This verifies user-facing
 * state changes without calling PostgreSQL, Rust, or OpenAI.
 */
describe('ChatWidget', () => {
  let fixture: ComponentFixture<ChatWidget>;
  let component: ChatWidget;

  let chatService: {
    getSessions: ReturnType<typeof vi.fn>;
    createSession: ReturnType<typeof vi.fn>;
    getSession: ReturnType<typeof vi.fn>;
    deleteSession: ReturnType<typeof vi.fn>;
    sendMessage: ReturnType<typeof vi.fn>;
  };

  const sessionOne: ChatSession = {
    id: 10,
    title: 'Monthly spending',
    created_at: '2026-11-16T12:00:00Z',
    updated_at: '2026-11-16T12:01:00Z',
  };

  const sessionTwo: ChatSession = {
    id: 20,
    title: 'Savings goals',
    created_at: '2026-11-16T13:00:00Z',
    updated_at: '2026-11-16T13:01:00Z',
  };

  const userMessage: ChatMessage = {
    id: 101,
    role: 'user',
    content: 'How much did I spend?',
    created_at: '2026-11-16T12:00:00Z',
  };

  const assistantMessage: ChatMessage = {
    id: 102,
    role: 'assistant',
    content: 'You spent $100.',
    created_at: '2026-11-16T12:01:00Z',
  };

  const savedSession: ChatSessionWithMessages = {
    ...sessionOne,
    messages: [userMessage, assistantMessage],
  };

  /**
   * A successful fake AI stream.
   *
   * The callback simulates the exact event sequence emitted by
   * Rust. The component must replace temporary text with the
   * database-backed conversation after completion.
   */
  const successfulStream = async (
    _sessionId: number,
    _message: string,
    onEvent: (event: ChatStreamEvent) => void,
  ): Promise<void> => {
    onEvent({ type: 'token', text: 'You spent ' });
    onEvent({ type: 'token', text: '$100.' });
    onEvent({ type: 'done', message: assistantMessage });
  };

  beforeEach(async () => {
    chatService = {
      getSessions: vi.fn().mockReturnValue(of({ sessions: [sessionOne, sessionTwo] })),
      createSession: vi.fn().mockReturnValue(of(sessionOne)),
      getSession: vi.fn().mockReturnValue(of(savedSession)),
      deleteSession: vi.fn().mockReturnValue(of(void 0)),
      sendMessage: vi.fn().mockImplementation(successfulStream),
    };

    await TestBed.configureTestingModule({
      imports: [ChatWidget],
      providers: [
        {
          provide: ChatService,
          useValue: chatService,
        },
      ],
    }).compileComponents();

    fixture = TestBed.createComponent(ChatWidget);
    component = fixture.componentInstance;

    // Triggers ngOnInit() and its initial history request.
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
  });

  // ---------------------------------------------------------
  // Window and navigation
  // ---------------------------------------------------------

  it('starts minimized with an empty active conversation', () => {
    expect(component.expanded()).toBe(false);
    expect(component.activeSessionId()).toBeNull();
    expect(component.messages()).toEqual([]);
    expect(component.sending()).toBe(false);

    const launcher: HTMLButtonElement | null =
      fixture.nativeElement.querySelector('.chat-launcher');

    expect(launcher).not.toBeNull();
  });

  it('expands and minimizes without losing conversation state', () => {
    component.activeSessionId.set(10);
    component.messages.set([userMessage]);

    component.toggleWindow();
    fixture.detectChanges();

    expect(component.expanded()).toBe(true);
    expect(fixture.nativeElement.querySelector('.chat-window')).not.toBeNull();

    component.toggleWindow();
    fixture.detectChanges();

    expect(component.expanded()).toBe(false);
    expect(component.activeSessionId()).toBe(10);
    expect(component.messages()).toEqual([userMessage]);
  });

  it('loads saved sessions when initialized', () => {
    expect(chatService.getSessions).toHaveBeenCalled();
    expect(component.sessions()).toEqual([sessionOne, sessionTwo]);
    expect(component.loadingSessions()).toBe(false);
  });

  it('opens history and renders conversation titles', async () => {
    component.toggleWindow();
    component.toggleHistory();

    await fixture.whenStable();
    fixture.detectChanges();

    expect(component.showingHistory()).toBe(true);

    const text = fixture.nativeElement.textContent as string;

    expect(text).toContain('Monthly spending');
    expect(text).toContain('Savings goals');
  });

  it('returns to the conversation after closing history', () => {
    component.toggleWindow();
    component.toggleHistory();
    component.toggleHistory();

    expect(component.showingHistory()).toBe(false);
    expect(component.expanded()).toBe(true);
  });

  it('displays a useful error when history cannot load', async () => {
    chatService.getSessions.mockReturnValue(throwError(() => new Error('Network unavailable')));

    await component.loadSessions();

    expect(component.loadingSessions()).toBe(false);
    expect(component.historyError()).toBe('Unable to load conversation history.');
  });

  it('starts a fresh conversation without creating a database row', () => {
    component.activeSessionId.set(10);
    component.messages.set([userMessage, assistantMessage]);
    component.draft.set('Old draft');
    component.showingHistory.set(true);

    component.newChat();

    expect(component.activeSessionId()).toBeNull();
    expect(component.messages()).toEqual([]);
    expect(component.draft()).toBe('');
    expect(component.showingHistory()).toBe(false);
    expect(chatService.createSession).not.toHaveBeenCalled();
  });

  it('opens a saved conversation and unlocks the composer', async () => {
    await component.openSession(10);

    expect(chatService.getSession).toHaveBeenCalledWith(10);
    expect(component.activeSessionId()).toBe(10);
    expect(component.messages()).toEqual(savedSession.messages);
    expect(component.loadingConversation()).toBe(false);
    expect(component.errorMessage()).toBeNull();
  });

  it('recovers when opening a saved conversation fails', async () => {
    chatService.getSession.mockReturnValue(throwError(() => new Error('Server unavailable')));

    await component.openSession(10);

    expect(component.errorMessage()).toBe('Unable to open this conversation.');

    // This assertion catches the missing finally-block bug.
    expect(component.loadingConversation()).toBe(false);
  });

  // ---------------------------------------------------------
  // Conversation deletion
  // ---------------------------------------------------------

  it('requires confirmation before deleting', () => {
    component.requestDelete(10);

    expect(component.confirmDeleteId()).toBe(10);
    expect(chatService.deleteSession).not.toHaveBeenCalled();
  });

  it('deletes the active conversation and clears its messages', async () => {
    component.activeSessionId.set(10);
    component.messages.set(savedSession.messages);
    component.requestDelete(10);

    await component.deleteSession(10);

    expect(chatService.deleteSession).toHaveBeenCalledWith(10);
    expect(component.activeSessionId()).toBeNull();
    expect(component.messages()).toEqual([]);
    expect(component.sessions()).toEqual([sessionTwo]);
    expect(component.confirmDeleteId()).toBeNull();
    expect(component.deletingSessionId()).toBeNull();
  });

  it('deletes another conversation without changing the active one', async () => {
    component.activeSessionId.set(10);
    component.messages.set(savedSession.messages);

    await component.deleteSession(20);

    expect(component.activeSessionId()).toBe(10);
    expect(component.messages()).toEqual(savedSession.messages);
    expect(component.sessions()).toEqual([sessionOne]);
  });

  it('keeps conversations intact when deletion fails', async () => {
    chatService.deleteSession.mockReturnValue(throwError(() => new Error('Database error')));

    await component.deleteSession(10);

    expect(component.sessions()).toEqual([sessionOne, sessionTwo]);
    expect(component.historyError()).toBe('Unable to delete this conversation.');
    expect(component.deletingSessionId()).toBeNull();
  });

  // ---------------------------------------------------------
  // Input validation and keyboard behavior
  // ---------------------------------------------------------

  it('does not send an empty or whitespace-only message', async () => {
    component.draft.set('   ');

    await component.send();

    expect(chatService.createSession).not.toHaveBeenCalled();
    expect(chatService.sendMessage).not.toHaveBeenCalled();
  });

  it('rejects messages longer than 2000 characters', async () => {
    component.draft.set('x'.repeat(2001));

    await component.send();

    expect(chatService.sendMessage).not.toHaveBeenCalled();
  });

  it('uses Enter to submit but allows Shift+Enter', () => {
    const sendSpy = vi.spyOn(component, 'send').mockResolvedValue();

    const enter = new KeyboardEvent('keydown', {
      key: 'Enter',
      cancelable: true,
    });

    component.onComposerKeydown(enter);

    expect(enter.defaultPrevented).toBe(true);
    expect(sendSpy).toHaveBeenCalledOnce();

    const shiftEnter = new KeyboardEvent('keydown', {
      key: 'Enter',
      shiftKey: true,
      cancelable: true,
    });

    component.onComposerKeydown(shiftEnter);

    expect(shiftEnter.defaultPrevented).toBe(false);
    expect(sendSpy).toHaveBeenCalledOnce();

    sendSpy.mockRestore();
  });

  // ---------------------------------------------------------
  // Successful streaming
  // ---------------------------------------------------------

  it('creates a session on the first message and saves the reply', async () => {
    component.draft.set('How much did I spend?');

    await component.send();

    expect(chatService.createSession).toHaveBeenCalledOnce();
    expect(chatService.sendMessage).toHaveBeenCalledOnce();

    expect(chatService.sendMessage).toHaveBeenCalledWith(
      10,
      'How much did I spend?',
      expect.any(Function),
      expect.any(AbortSignal),
    );

    expect(component.activeSessionId()).toBe(10);
    expect(component.messages()).toEqual(savedSession.messages);
    expect(component.pendingMessage()).toBeNull();
    expect(component.streamedText()).toBe('');
    expect(component.draft()).toBe('');
    expect(component.sending()).toBe(false);
  });

  it('reuses an existing session for subsequent messages', async () => {
    component.activeSessionId.set(10);
    component.draft.set('Tell me more');

    await component.send();

    expect(chatService.createSession).not.toHaveBeenCalled();
    expect(chatService.sendMessage).toHaveBeenCalledWith(
      10,
      'Tell me more',
      expect.any(Function),
      expect.any(AbortSignal),
    );
  });

  it('displays real tool activity and partial tokens while streaming', async () => {
    let finishStream!: () => void;

    const pendingStream = new Promise<void>((resolve) => {
      finishStream = resolve;
    });

    chatService.sendMessage.mockImplementation(
      async (_id: number, _text: string, onEvent: (event: ChatStreamEvent) => void) => {
        onEvent({ type: 'status', tool: 'web_search' });
        onEvent({ type: 'status', tool: 'web_search' });
        onEvent({ type: 'token', text: 'Hello ' });
        onEvent({ type: 'token', text: 'Arthur' });

        await pendingStream;

        onEvent({ type: 'done', message: assistantMessage });
      },
    );

    component.activeSessionId.set(10);
    component.draft.set('Hello');

    const sendPromise = component.send();

    await vi.waitFor(() => {
      expect(chatService.sendMessage).toHaveBeenCalledOnce();
    });

    expect(component.sending()).toBe(true);
    expect(component.pendingMessage()).toBe('Hello');
    expect(component.streamedText()).toBe('Hello Arthur');
    expect(component.activity()).toEqual(['Searching the web...']);

    finishStream();
    await sendPromise;

    expect(component.sending()).toBe(false);
    expect(component.streamedText()).toBe('');
    expect(component.pendingMessage()).toBeNull();
  });

  it('prevents duplicate submissions during an active response', async () => {
    let finishStream!: () => void;

    chatService.sendMessage.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          finishStream = resolve;
        }),
    );

    component.activeSessionId.set(10);
    component.draft.set('First question');

    const firstRequest = component.send();

    await vi.waitFor(() => {
      expect(chatService.sendMessage).toHaveBeenCalledOnce();
    });

    component.draft.set('Second question');
    await component.send();

    expect(chatService.sendMessage).toHaveBeenCalledOnce();

    // Finish the first request and restore normal interaction.
    finishStream();
    await firstRequest;

    expect(component.sending()).toBe(false);
  });

  // ---------------------------------------------------------
  // Error recovery
  // ---------------------------------------------------------

  it('preserves the question when the AI request fails', async () => {
    chatService.sendMessage.mockRejectedValue(new Error('Assistant unavailable'));

    component.activeSessionId.set(10);
    component.draft.set('What is my balance?');

    await component.send();

    expect(component.errorMessage()).toBe('Assistant unavailable');
    expect(component.failedMessage()).toBe('What is my balance?');
    expect(component.draft()).toBe('What is my balance?');
    expect(component.pendingMessage()).toBeNull();
    expect(component.sending()).toBe(false);
  });

  it('preserves the draft if creating a session fails', async () => {
    chatService.createSession.mockReturnValue(throwError(() => new Error('Cannot create session')));

    component.draft.set('New question');

    await component.send();

    expect(component.errorMessage()).toBe('Cannot create session');
    expect(component.draft()).toBe('New question');
    expect(component.failedMessage()).toBe('New question');
    expect(chatService.sendMessage).not.toHaveBeenCalled();
  });

  it('keeps a confirmed reply if history refresh fails', async () => {
    chatService.getSession.mockReturnValue(throwError(() => new Error('History request failed')));

    component.activeSessionId.set(10);
    component.draft.set('How much did I spend?');

    await component.send();

    const messages = component.messages();

    expect(messages).toHaveLength(2);
    expect(messages[0].role).toBe('user');
    expect(messages[0].content).toBe('How much did I spend?');
    expect(messages[0].id).toBeLessThan(0);
    expect(messages[1]).toEqual(assistantMessage);

    expect(component.failedMessage()).toBeNull();
    expect(component.draft()).toBe('');
    expect(component.errorMessage()).toContain('Your answer was saved');
  });

  it('clears an earlier error when starting a new chat', () => {
    component.errorMessage.set('Previous error');
    component.failedMessage.set('Failed question');
    component.draft.set('Failed question');

    component.newChat();

    expect(component.errorMessage()).toBeNull();
    expect(component.failedMessage()).toBeNull();
    expect(component.draft()).toBe('');
  });

  /**
   * A new chat must not replace the current conversation while
   * an AI response is still being generated.
   */
  it('prevents starting a new chat during streaming', () => {
    component.activeSessionId.set(10);
    component.messages.set([userMessage]);
    component.sending.set(true);

    component.newChat();

    expect(component.activeSessionId()).toBe(10);
    expect(component.messages()).toEqual([userMessage]);
  });

  /**
   * Loading an existing session should not interrupt an active
   * AI response or launch another request.
   */
  it('prevents opening another conversation while sending', async () => {
    component.sending.set(true);

    await component.openSession(20);

    expect(chatService.getSession).not.toHaveBeenCalled();
    expect(component.activeSessionId()).toBeNull();
  });

  /**
   * A conversation being loaded must finish before the user
   * can start a different conversation.
   */
  it('prevents starting a new chat while loading a conversation', () => {
    component.activeSessionId.set(10);
    component.messages.set([userMessage]);
    component.loadingConversation.set(true);

    component.newChat();

    expect(component.activeSessionId()).toBe(10);
    expect(component.messages()).toEqual([userMessage]);
  });

  /**
   * Prevent deleting a conversation while the assistant is
   * working, avoiding conflicting UI operations.
   */
  it('blocks delete confirmation during streaming', () => {
    component.sending.set(true);

    component.requestDelete(10);

    expect(component.confirmDeleteId()).toBeNull();
    expect(chatService.deleteSession).not.toHaveBeenCalled();
  });
});
