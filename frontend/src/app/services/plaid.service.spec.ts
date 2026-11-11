import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';

import { PlaidService } from './plaid.service';

describe('PlaidService', () => {
  let service: PlaidService;
  let httpMock: HttpTestingController;

  const baseUrl = 'http://localhost:3001/api/plaid';

  beforeEach(() => {
    // Create an Angular testing environment with a fake HTTP backend.
    TestBed.configureTestingModule({
      providers: [PlaidService, provideHttpClient(), provideHttpClientTesting()],
    });

    service = TestBed.inject(PlaidService);
    httpMock = TestBed.inject(HttpTestingController);
  });

  afterEach(() => {
    // Fail if a test leaves any unexpected HTTP requests unfinished.
    httpMock.verify();
  });

  it('retrieves the authenticated user accounts', () => {
    // Call our service and capture the response.
    let accountCount = -1;

    service.getAccounts().subscribe((response) => {
      accountCount = response.accounts.length;
    });

    // Verify the method sends the expected HTTP request.
    const request = httpMock.expectOne(`${baseUrl}/accounts`);

    expect(request.request.method).toBe('GET');
    expect(request.request.withCredentials).toBe(true);

    // Pretend the Rust backend returned one account.
    request.flush({
      accounts: [
        {
          account_id: 'acct-123',
          item_id: 'item-456',
          name: 'Checking',
          official_name: null,
          account_type: 'depository',
          account_subtype: 'checking',
          mask: '1234',
          current_balance: 250,
          available_balance: 200,
          currency_code: 'USD',
        },
      ],
    });

    expect(accountCount).toBe(1);
  });

  it('requests a Plaid Link token', () => {
    let linkToken = '';

    service.createLinkToken().subscribe((response) => {
      linkToken = response.link_token;
    });

    const request = httpMock.expectOne(`${baseUrl}/link-token`);

    expect(request.request.method).toBe('POST');
    expect(request.request.body).toEqual({});
    expect(request.request.withCredentials).toBe(true);

    // Simulate Rust returning a Link token.
    request.flush({ link_token: 'link-sandbox-test' });

    expect(linkToken).toBe('link-sandbox-test');
  });

  it('exchanges a public token through the backend', () => {
    let connectedItem = '';

    service.exchangeToken('public-test').subscribe((response) => {
      connectedItem = response.item_id;
    });

    const request = httpMock.expectOne(`${baseUrl}/exchange-token`);

    expect(request.request.method).toBe('POST');
    expect(request.request.body).toEqual({
      public_token: 'public-test',
    });
    expect(request.request.withCredentials).toBe(true);

    // Simulate a successful bank connection.
    request.flush({
      success: true,
      item_id: 'item-456',
    });

    expect(connectedItem).toBe('item-456');
  });

  it('disconnects a bank using an authenticated DELETE request', () => {
    let completed = false;

    service.removeItem('item-456').subscribe(() => {
      completed = true;
    });

    const request = httpMock.expectOne(`${baseUrl}/items/item-456`);

    expect(request.request.method).toBe('DELETE');
    expect(request.request.withCredentials).toBe(true);

    // Our Rust controller responds with HTTP 204 No Content.
    request.flush(null, {
      status: 204,
      statusText: 'No Content',
    });

    expect(completed).toBe(true);
  });

  it('retrieves transactions for the selected account', () => {
    let transactionCount = -1;

    service.getTransactions('account-123', 20, 0).subscribe((response) => {
      transactionCount = response.transactions.length;
    });

    // Verify the account ID and pagination are sent to Rust.
    const request = httpMock.expectOne((req) => req.url === `${baseUrl}/transactions`);

    expect(request.request.method).toBe('GET');
    expect(request.request.withCredentials).toBe(true);
    expect(request.request.params.get('account_id')).toBe('account-123');
    expect(request.request.params.get('limit')).toBe('20');
    expect(request.request.params.get('offset')).toBe('0');

    request.flush({
      transactions: [
        {
          transaction_id: 'tx-1',
          account_id: 'account-123',
          name: 'Grocery Store',
          amount: 50,
          date: '2026-11-10',
          pending: false,
          category: 'GROCERIES',
        },
      ],
      total: 1,
      limit: 20,
      offset: 0,
    });

    expect(transactionCount).toBe(1);
  });

  it('supports transaction pagination', () => {
    service.getTransactions('account-123', 20, 20).subscribe();

    const request = httpMock.expectOne((req) => req.url === `${baseUrl}/transactions`);

    expect(request.request.params.get('offset')).toBe('20');
    expect(request.request.params.get('limit')).toBe('20');

    request.flush({
      transactions: [],
      total: 20,
      limit: 20,
      offset: 20,
    });
  });

  it('can request transactions across all accounts', () => {
    service.getTransactions().subscribe();

    const request = httpMock.expectOne((req) => req.url === `${baseUrl}/transactions`);

    expect(request.request.params.has('account_id')).toBe(false);

    request.flush({
      transactions: [],
      total: 0,
      limit: 20,
      offset: 0,
    });
  });

  it('retrieves historical net-worth snapshots', () => {
    let snapshotCount = -1;

    service.getNetWorthHistory().subscribe((response) => {
      snapshotCount = response.snapshots.length;
    });

    /**
     * Net-worth history now belongs to the account API,
     * rather than the Plaid integration API.
     */
    const request = httpMock.expectOne('http://localhost:3001/api/accounts/net-worth/history');

    expect(request.request.method).toBe('GET');
    expect(request.request.withCredentials).toBe(true);

    // Simulate real historical data returned by Rust.
    request.flush({
      snapshots: [
        {
          snapshot_date: '2026-11-10',
          amount: 12500,
          included_accounts: 3,
          excluded_accounts: 0,
        },
      ],
    });

    expect(snapshotCount).toBe(1);
  });
});
