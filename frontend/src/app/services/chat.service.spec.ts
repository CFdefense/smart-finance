import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ChatService } from './chat.service';
import type { ChatMessage, ChatSession, ChatStreamEvent } from '../models/chat.models';

/**
 * ChatService tests.
 *
 * REST requests use Angular's fake HTTP backend.
 * Streaming requests use a mocked fetch() and ReadableStream.
 *
 * No test contacts Rust, PostgreSQL, Python, or OpenAI.
 */
describe('ChatService', () => {
  let service: ChatService;
  let httpMock: HttpTestingController;

  const baseUrl = 'http://localhost:3001/api/chat';

  const mockSession: ChatSession = {
    id: 12,
    title: 'Monthly spending',
    created_at: '2026-11-16T12:00:00Z',
    updated_at: '2026-11-16T12:01:00Z',
  };

  const mockMessages: ChatMessage[] = [
    {
      id: 101,
      role: 'user',
      content: 'How much did I spend?',
      created_at: '2026-11-16T12:00:00Z',
    },
    {
      id: 102,
      role: 'assistant',
      content: 'You spent $100.',
      created_at: '2026-11-16T12:01:00Z',
    },
  ];

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [ChatService, provideHttpClient(), provideHttpClientTesting()],
    });

    service = TestBed.inject(ChatService);
    httpMock = TestBed.inject(HttpTestingController);
  });

  afterEach(() => {
    // Ensure no Angular HTTP requests were left outstanding.
    httpMock.verify();

    // Prevent mocked fetch() from affecting other test files.
    vi.unstubAllGlobals();
  });

  // ---------------------------------------------------------
  // REST API tests
  // ---------------------------------------------------------

  it('lists the authenticated user conversations', () => {
    let received: ChatSession[] = [];

    service.getSessions().subscribe((result) => {
      received = result.sessions;
    });

    const request = httpMock.expectOne(`${baseUrl}/sessions`);

    expect(request.request.method).toBe('GET');
    expect(request.request.withCredentials).toBe(true);

    request.flush({ sessions: [mockSession] });

    expect(received).toEqual([mockSession]);
  });

  it('creates a new conversation', () => {
    let created: ChatSession | undefined;

    service.createSession().subscribe((session) => {
      created = session;
    });

    const request = httpMock.expectOne(`${baseUrl}/sessions`);

    expect(request.request.method).toBe('POST');
    expect(request.request.withCredentials).toBe(true);
    expect(request.request.body).toEqual({});

    request.flush(mockSession, {
      status: 201,
      statusText: 'Created',
    });

    expect(created).toEqual(mockSession);
  });

  it('retrieves a session with its saved messages', () => {
    let messages: ChatMessage[] = [];

    service.getSession(12).subscribe((session) => {
      messages = session.messages;
    });

    const request = httpMock.expectOne(`${baseUrl}/sessions/12`);

    expect(request.request.method).toBe('GET');
    expect(request.request.withCredentials).toBe(true);

    request.flush({
      ...mockSession,
      messages: mockMessages,
    });

    expect(messages).toEqual(mockMessages);
  });

  it('deletes a conversation using the authenticated API', () => {
    let completed = false;

    service.deleteSession(12).subscribe(() => {
      completed = true;
    });

    const request = httpMock.expectOne(`${baseUrl}/sessions/12`);

    expect(request.request.method).toBe('DELETE');
    expect(request.request.withCredentials).toBe(true);

    request.flush(null, {
      status: 204,
      statusText: 'No Content',
    });

    expect(completed).toBe(true);
  });

  // ---------------------------------------------------------
  // Streaming test helpers
  // ---------------------------------------------------------

  /**
   * Build a fake HTTP streaming response from network chunks.
   *
   * Each chunk is encoded separately, allowing tests to verify
   * that the parser handles arbitrary chunk boundaries.
   */
  function streamResponse(chunks: string[]): Response {
    const encoder = new TextEncoder();

    const stream = new ReadableStream<Uint8Array>({
      start(controller) {
        for (const chunk of chunks) {
          controller.enqueue(encoder.encode(chunk));
        }

        controller.close();
      },
    });

    return new Response(stream, {
      status: 200,
      headers: {
        'Content-Type': 'text/event-stream',
      },
    });
  }

  /** Build a named SSE event using the Rust API format. */
  function sse(name: string, payload: Record<string, unknown>): string {
    return `event: ${name}\ndata: ${JSON.stringify(payload)}\n\n`;
  }

  /** Replace global fetch with a predictable fake response. */
  function mockFetch(response: Response): ReturnType<typeof vi.fn> {
    const fetchMock = vi.fn().mockResolvedValue(response);
    vi.stubGlobal('fetch', fetchMock);
    return fetchMock;
  }

  // ---------------------------------------------------------
  // SSE streaming tests
  // ---------------------------------------------------------

  it('streams status, tokens, and the saved completion event', async () => {
    const events: ChatStreamEvent[] = [];

    const responseText =
      sse('status', { tool: 'web_search' }) +
      sse('token', { text: 'Hello ' }) +
      sse('token', { text: 'Arthur!' }) +
      sse('done', { message: mockMessages[1] });

    const fetchMock = mockFetch(streamResponse([responseText]));

    await service.sendMessage(12, 'Hello', (event) => events.push(event));

    expect(fetchMock).toHaveBeenCalledOnce();

    const [url, options] = fetchMock.mock.calls[0] as [string, RequestInit];

    expect(url).toBe(`${baseUrl}/sessions/12/messages`);
    expect(options.method).toBe('POST');
    expect(options.credentials).toBe('include');
    expect(options.body).toBe(JSON.stringify({ message: 'Hello' }));

    expect(events).toEqual([
      { type: 'status', tool: 'web_search' },
      { type: 'token', text: 'Hello ' },
      { type: 'token', text: 'Arthur!' },
      { type: 'done', message: mockMessages[1] },
    ]);
  });

  it('reconstructs SSE events split across network chunks', async () => {
    const events: ChatStreamEvent[] = [];

    // These are deliberately NOT complete lines or events.
    const chunks = [
      'event: tok',
      'en\ndata: {"te',
      'xt":"Hello"}\n',
      '\n',
      'event: done\n',
      `data: ${JSON.stringify({ message: mockMessages[1] })}`,
      '\n\n',
    ];

    mockFetch(streamResponse(chunks));

    await service.sendMessage(12, 'Hello', (event) => events.push(event));

    expect(events).toEqual([
      { type: 'token', text: 'Hello' },
      { type: 'done', message: mockMessages[1] },
    ]);
  });

  it('ignores SSE keepalive comments', async () => {
    const events: ChatStreamEvent[] = [];

    mockFetch(
      streamResponse([
        ': keep-alive\n\n',
        sse('token', { text: 'Hi' }),
        ': another keep-alive\n\n',
        sse('done', { message: mockMessages[1] }),
      ]),
    );

    await service.sendMessage(12, 'Hello', (event) => events.push(event));

    expect(events).toHaveLength(2);
    expect(events[0]).toEqual({ type: 'token', text: 'Hi' });
    expect(events[1].type).toBe('done');
  });

  it('rejects an error event emitted during streaming', async () => {
    const events: ChatStreamEvent[] = [];

    mockFetch(
      streamResponse([
        sse('token', { text: 'Partial answer' }),
        sse('error', {
          code: 'ai_failed',
          message: 'The AI service failed.',
        }),
      ]),
    );

    await expect(service.sendMessage(12, 'Hello', (event) => events.push(event))).rejects.toThrow(
      'The AI service failed.',
    );

    expect(events).toEqual([
      { type: 'token', text: 'Partial answer' },
      {
        type: 'error',
        code: 'ai_failed',
        message: 'The AI service failed.',
      },
    ]);
  });

  it('rejects a stream that ends without a done event', async () => {
    mockFetch(streamResponse([sse('token', { text: 'Incomplete answer' })]));

    await expect(service.sendMessage(12, 'Hello', () => undefined)).rejects.toThrow(
      'The response ended before it was saved.',
    );
  });

  it('handles HTTP 409 when another response is in progress', async () => {
    mockFetch(
      new Response(null, {
        status: 409,
        statusText: 'Conflict',
      }),
    );

    await expect(service.sendMessage(12, 'Hello', () => undefined)).rejects.toThrow(
      'A response is already being generated.',
    );
  });

  it('handles HTTP 503 when the assistant is unavailable', async () => {
    mockFetch(
      new Response(null, {
        status: 503,
        statusText: 'Service Unavailable',
      }),
    );

    await expect(service.sendMessage(12, 'Hello', () => undefined)).rejects.toThrow(
      'The AI assistant is currently unavailable.',
    );
  });

  it('rejects a successful HTTP response without a body', async () => {
    mockFetch(
      new Response(null, {
        status: 200,
      }),
    );

    await expect(service.sendMessage(12, 'Hello', () => undefined)).rejects.toThrow(
      'The server did not provide a response stream.',
    );
  });
});
