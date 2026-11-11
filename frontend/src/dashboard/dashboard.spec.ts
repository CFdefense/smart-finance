import { TestBed } from '@angular/core/testing';
import { provideRouter, Router } from '@angular/router';
import { of, throwError } from 'rxjs';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { UserDashboard } from './dashboard';
import { AuthService } from '../app/services/auth.service';
import { ProfileService } from '../app/services/profile.service';
import { BudgetService, type Budget } from '../app/services/budget.service';
import { PlaidService, type PlaidAccount } from '../app/services/plaid.service';

describe('UserDashboard', () => {
  /**
   * Returns a date a specified number of days before today.
   *
   * This prevents historical chart tests from breaking
   * whenever the calendar date changes.
   *
   * Examples:
   * daysAgo(0) = today
   * daysAgo(1) = yesterday
   * daysAgo(10) = ten days ago
   */
  const daysAgo = (days: number): string => {
    const date = new Date();

    // Use UTC consistently with YYYY-MM-DD snapshot strings.
    date.setUTCHours(12, 0, 0, 0);
    date.setUTCDate(date.getUTCDate() - days);

    return date.toISOString().slice(0, 10);
  };

  let component: UserDashboard;
  let router: Router;

  // Replace real services with predictable test doubles.
  let profileService: {
    getProfile: ReturnType<typeof vi.fn>;
  };

  let budgetService: {
    getBudgets: ReturnType<typeof vi.fn>;
    getSpendingByCategory: ReturnType<typeof vi.fn>;
  };

  let authService: {
    logout: ReturnType<typeof vi.fn>;
  };

  // The Rust API already calculates individual budget spending.
  const mockBudgets: Budget[] = [
    {
      id: 1,
      name: 'Groceries',
      category: 'GROCERIES',
      monthly_limit: 400,
      active: true,
      month: '2026-11',
      spent: 150,
      remaining: 250,
      percent_used: 37.5,
      over_budget: false,
    },
    {
      id: 2,
      name: 'Entertainment',
      category: 'ENTERTAINMENT',
      monthly_limit: 200,
      active: true,
      month: '2026-11',
      spent: 50,
      remaining: 150,
      percent_used: 25,
      over_budget: false,
    },
    {
      id: 3,
      name: 'Shopping',
      category: 'SHOPPING',
      monthly_limit: 300,
      active: false,
      month: '2026-11',
      spent: 100,
      remaining: 200,
      percent_used: 33.33,
      over_budget: false,
    },
  ];

  let plaidService: {
    getAccounts: ReturnType<typeof vi.fn>;
    getTransactions: ReturnType<typeof vi.fn>;
    getNetWorthHistory: ReturnType<typeof vi.fn>;
  };

  const mockAccounts: PlaidAccount[] = [
    {
      account_id: 'account-123',
      item_id: 'item-123',
      institution_name: 'Test Bank',
      name: 'Checking',
      official_name: null,
      account_type: 'depository',
      account_subtype: 'checking',
      mask: '1234',
      current_balance: 2000,
      available_balance: 1800,
      currency_code: 'USD',
    },
  ];

  beforeEach(async () => {
    profileService = {
      getProfile: vi.fn().mockReturnValue(of({ first_name: 'Mitchell' })),
    };

    budgetService = {
      // Existing budget API mock.
      getBudgets: vi.fn().mockReturnValue(of({ month: '2026-11', budgets: mockBudgets })),

      // New monthly category-spending API mock.
      getSpendingByCategory: vi.fn().mockReturnValue(
        of({
          month: '2026-11',
          total: 100,
          categories: [
            { category: 'GROCERIES', amount: 60 },
            { category: 'FOOD_DINING', amount: 40 },
          ],
        }),
      ),
    };

    authService = {
      logout: vi.fn().mockReturnValue(of(void 0)),
    };

    plaidService = {
      getAccounts: vi.fn().mockReturnValue(of({ accounts: mockAccounts })),

      getTransactions: vi.fn().mockReturnValue(
        of({
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
        }),
      ),

      getNetWorthHistory: vi.fn().mockReturnValue(
        of({
          snapshots: [
            {
              snapshot_date: '2026-11-10',
              amount: 2000,
              included_accounts: 1,
              excluded_accounts: 0,
            },
          ],
        }),
      ),
    };

    await TestBed.configureTestingModule({
      imports: [UserDashboard],
      providers: [
        provideRouter([]),
        { provide: ProfileService, useValue: profileService },
        { provide: BudgetService, useValue: budgetService },
        { provide: AuthService, useValue: authService },
        { provide: PlaidService, useValue: plaidService },
      ],
    })
      // These tests target dashboard logic, not HTML rendering.
      // This keeps them focused on API data and navigation.
      .overrideComponent(UserDashboard, {
        set: { template: '' },
      })
      .compileComponents();

    router = TestBed.inject(Router);
    vi.spyOn(router, 'navigate').mockResolvedValue(true);

    const fixture = TestBed.createComponent(UserDashboard);
    component = fixture.componentInstance;

    // ngOnInit() retrieves the profile and budget summary.
    fixture.detectChanges();
  });

  /**
   * Verifies that the dashboard retrieves the user's first name.
   */
  it('should load the signed-in user name', () => {
    expect(profileService.getProfile).toHaveBeenCalledOnce();
    expect(component.firstName()).toBe('Mitchell');
    expect(component.profileLoading()).toBe(false);
  });

  /**
   * Verifies that the dashboard loads budgets on initialization.
   */
  it('should load current budget data', () => {
    expect(budgetService.getBudgets).toHaveBeenCalledOnce();
    expect(component.budgets()).toEqual(mockBudgets);
    expect(component.budgetsLoading()).toBe(false);
  });

  /**
   * Verifies that inactive budgets do not count toward the total.
   */
  it('should calculate total budget from active budgets', () => {
    // 400 + 200; excludes inactive Shopping budget.
    expect(component.totalBudget()).toBe(600);
  });

  /**
   * Verifies that spending is summed across active categories.
   */
  it('should calculate actual spending', () => {
    // 150 + 50; excludes inactive Shopping spending.
    expect(component.totalSpent()).toBe(200);
  });

  /**
   * Verifies that remaining money is calculated correctly.
   */
  it('should calculate remaining budget', () => {
    expect(component.totalRemaining()).toBe(400);
  });

  /**
   * Verifies that financial totals react to new budget data.
   */
  it('should recalculate when budgets change', () => {
    component.budgets.set([{ ...mockBudgets[0], monthly_limit: 800 }]);

    expect(component.totalBudget()).toBe(800);
    expect(component.totalSpent()).toBe(150);
    expect(component.totalRemaining()).toBe(650);
  });

  /**
   * Verifies that profile failures do not display a fake name.
   */
  it('should handle a profile loading failure', () => {
    profileService.getProfile.mockReturnValue(throwError(() => new Error('Profile unavailable')));

    // Create another dashboard with the failing service.
    const fixture = TestBed.createComponent(UserDashboard);
    fixture.detectChanges();

    expect(fixture.componentInstance.firstName()).toBe('');
    expect(fixture.componentInstance.profileLoading()).toBe(false);
  });

  /**
   * Verifies that a failed request is not mistaken for zero spending.
   */
  it('should report a budget loading failure', () => {
    budgetService.getBudgets.mockReturnValue(throwError(() => new Error('Budget API unavailable')));

    const fixture = TestBed.createComponent(UserDashboard);
    fixture.detectChanges();

    expect(fixture.componentInstance.budgetsError()).toBe(true);
    expect(fixture.componentInstance.budgetsLoading()).toBe(false);
  });

  /**
   * Verifies that the Budgets shortcut opens the Budgets page.
   */
  it('should navigate to Budgets', () => {
    component.openBudgets();

    expect(router.navigate).toHaveBeenCalledWith(['/budgets']);
  });

  /**
   * Verifies that the Goals shortcut opens the Goals page.
   */
  it('should navigate to Goals', () => {
    component.openGoals();

    expect(router.navigate).toHaveBeenCalledWith(['/goals']);
  });

  /**
   * Verifies that successful logout returns to Login.
   */
  it('should navigate to Login after logout', () => {
    component.onLogout();

    expect(authService.logout).toHaveBeenCalledOnce();
    expect(router.navigate).toHaveBeenCalledWith(['/login']);
  });

  /**
   * Verifies that the profile dropdown opens the Profile page.
   *
   * This preserves the navigation test from PR #72.
   */
  it('should navigate to Profile', () => {
    component.openProfile();

    expect(router.navigate).toHaveBeenCalledWith(['/profile']);
  });

  it('loads connected accounts when dashboard initializes', () => {
    expect(plaidService.getAccounts).toHaveBeenCalledOnce();
    expect(component.accounts()).toEqual(mockAccounts);
    expect(component.accountsLoading()).toBe(false);
  });

  it('loads transactions when an account is selected', () => {
    component.selectAccount('account-123');

    expect(component.selectedAccountId()).toBe('account-123');

    expect(plaidService.getTransactions).toHaveBeenCalledWith('account-123', 20, 0);

    expect(component.transactions()).toHaveLength(1);
    expect(component.transactionTotal()).toBe(1);
  });

  it('collapses an account when selected twice', () => {
    component.selectAccount('account-123');
    component.selectAccount('account-123');

    expect(component.selectedAccountId()).toBeNull();
    expect(component.transactions()).toEqual([]);
    expect(component.transactionsLoading()).toBe(false);
  });

  it('handles an account retrieval failure', () => {
    plaidService.getAccounts.mockReturnValue(throwError(() => new Error('Accounts unavailable')));

    const fixture = TestBed.createComponent(UserDashboard);
    fixture.detectChanges();

    expect(fixture.componentInstance.accountsError()).toBe(true);
    expect(fixture.componentInstance.accountsLoading()).toBe(false);
  });

  it('handles a transaction retrieval failure', () => {
    plaidService.getTransactions.mockReturnValue(
      throwError(() => new Error('Transactions unavailable')),
    );

    component.selectAccount('account-123');

    expect(component.transactionsError()).toBe(true);
    expect(component.transactionsLoading()).toBe(false);
  });

  it('loads net-worth history after loading accounts', () => {
    expect(plaidService.getAccounts).toHaveBeenCalledOnce();
    expect(plaidService.getNetWorthHistory).toHaveBeenCalledOnce();

    expect(component.netWorthHistory()).toHaveLength(1);
    expect(component.netWorthHistoryLoading()).toBe(false);
  });

  it('creates a chart marker for one historical snapshot', () => {
    const chart = component.netWorthChart();

    expect(chart.markers).toHaveLength(1);
    expect(chart.markers[0].amount).toBe(2000);

    // A single point is centered rather than stretched
    // across an imaginary historical timeline.
    expect(chart.markers[0].x).toBe(320);
  });

  it('plots multiple historical snapshots in date order', () => {
    component.netWorthHistory.set([
      {
        // Two days ago, net worth was $1,000.
        snapshot_date: daysAgo(2),
        amount: 1000,
        included_accounts: 1,
        excluded_accounts: 0,
      },
      {
        // Today, net worth increased to $1,500.
        snapshot_date: daysAgo(0),
        amount: 1500,
        included_accounts: 1,
        excluded_accounts: 0,
      },
    ]);

    const chart = component.netWorthChart();

    // Both snapshots should appear in chronological order.
    expect(chart.markers).toHaveLength(2);
    expect(chart.firstDate).toBe(daysAgo(2));
    expect(chart.lastDate).toBe(daysAgo(0));

    // SVG coordinates increase downward.
    // A higher net worth must therefore have a smaller Y value.
    expect(chart.markers[1].y).toBeLessThan(chart.markers[0].y);
  });

  it('handles an empty net-worth history', () => {
    component.netWorthHistory.set([]);

    expect(component.netWorthChart().markers).toEqual([]);
    expect(component.netWorthChart().points).toBe('');
  });

  it('handles failure when retrieving net-worth history', () => {
    plaidService.getNetWorthHistory.mockReturnValue(
      throwError(() => new Error('History unavailable')),
    );

    component.loadNetWorthHistory();

    expect(component.netWorthHistoryError()).toBe(true);
    expect(component.netWorthHistoryLoading()).toBe(false);
  });

  /**
   * Verify that a constant net worth doesn't cause
   * division by zero when calculating SVG coordinates.
   */
  it('plots equal net-worth values safely', () => {
    component.netWorthHistory.set([
      {
        snapshot_date: '2026-11-10',
        amount: 2000,
        included_accounts: 1,
        excluded_accounts: 0,
      },
      {
        snapshot_date: '2026-11-11',
        amount: 2000,
        included_accounts: 1,
        excluded_accounts: 0,
      },
    ]);

    const chart = component.netWorthChart();

    expect(chart.markers).toHaveLength(2);
    expect(chart.markers[0].y).toBe(chart.markers[1].y);
    expect(Number.isFinite(chart.markers[0].y)).toBe(true);
  });

  /**
   * Verify that gaps in calendar dates are proportional.
   *
   * A snapshot on November 11 should be much closer to
   * November 10 than November 20.
   */
  it('spaces historical points according to actual dates', () => {
    component.netWorthHistory.set([
      {
        // First snapshot: 20 days ago
        snapshot_date: daysAgo(20),
        amount: 1000,
        included_accounts: 1,
        excluded_accounts: 0,
      },
      {
        // Second snapshot: 10 days ago
        snapshot_date: daysAgo(10),
        amount: 1200,
        included_accounts: 1,
        excluded_accounts: 0,
      },
      {
        // Third snapshot: today
        snapshot_date: daysAgo(0),
        amount: 1500,
        included_accounts: 1,
        excluded_accounts: 0,
      },
    ]);

    const markers = component.netWorthChart().markers;

    expect(markers).toHaveLength(3);
    expect(markers[0].x).toBe(50);
    expect(markers[2].x).toBe(590);

    // The middle date is exactly halfway between
    // the earliest and latest dates.
    //
    // Chart boundaries: x = 50 and x = 590.
    // Their midpoint is 320.
    expect(markers[1].x).toBeCloseTo(320);
  });

  /**
   * Negative net worth is valid when liabilities exceed assets.
   */
  it('supports negative historical net worth', () => {
    component.netWorthHistory.set([
      {
        snapshot_date: '2026-11-10',
        amount: -3000,
        included_accounts: 2,
        excluded_accounts: 0,
      },
      {
        snapshot_date: '2026-11-11',
        amount: -2000,
        included_accounts: 2,
        excluded_accounts: 0,
      },
    ]);

    const chart = component.netWorthChart();

    expect(chart.minValue).toBeLessThan(-3000);
    expect(chart.maxValue).toBeGreaterThan(-2000);
    expect(chart.markers[1].y).toBeLessThan(chart.markers[0].y);
  });

  /**
   * A selected account should always begin at page zero.
   */
  it('resets pagination when switching accounts', () => {
    component.selectAccount('account-123');

    component.transactionPage.set(3);
    component.selectAccount('account-456');

    expect(component.selectedAccountId()).toBe('account-456');
    expect(component.transactionPage()).toBe(0);

    expect(plaidService.getTransactions).toHaveBeenLastCalledWith('account-456', 20, 0);
  });

  /**
   * Test the page-navigation guards.
   */
  it('does not navigate before the first transaction page', () => {
    component.selectAccount('account-123');
    plaidService.getTransactions.mockClear();

    component.previousTransactionPage();

    expect(component.transactionPage()).toBe(0);
    expect(plaidService.getTransactions).not.toHaveBeenCalled();
  });

  it('does not navigate beyond the final transaction page', () => {
    component.selectAccount('account-123');
    plaidService.getTransactions.mockClear();

    // Our default mock response contains only one transaction.
    component.nextTransactionPage();

    expect(component.transactionPage()).toBe(0);
    expect(plaidService.getTransactions).not.toHaveBeenCalled();
  });

  /**
   * Verify that the Next button requests the correct
   * database offset when there are multiple pages.
   */
  it('loads the next transaction page when available', () => {
    component.selectAccount('account-123');

    // Simulate three pages of transactions.
    component.transactionTotal.set(45);

    component.nextTransactionPage();

    expect(component.transactionPage()).toBe(1);
    expect(plaidService.getTransactions).toHaveBeenLastCalledWith('account-123', 20, 20);
  });

  it('returns to the previous transaction page', () => {
    component.selectAccount('account-123');
    component.transactionTotal.set(45);

    component.nextTransactionPage();
    component.previousTransactionPage();

    expect(component.transactionPage()).toBe(0);
    expect(plaidService.getTransactions).toHaveBeenLastCalledWith('account-123', 20, 0);
  });

  it('does not load transactions without a selected account', () => {
    plaidService.getTransactions.mockClear();

    component.loadTransactions();

    expect(plaidService.getTransactions).not.toHaveBeenCalled();
  });

  /**
   * Verify that unknown account types aren't incorrectly
   * counted as assets or liabilities.
   */
  it('excludes unsupported account types from net worth', () => {
    component.accounts.set([
      {
        ...mockAccounts[0],
        account_type: 'other',
        current_balance: 5000,
      },
    ]);

    const result = component.netWorthSummary();

    expect(result.amount).toBe(0);
    expect(result.included).toBe(0);
    expect(result.excluded).toBe(1);
  });

  it('excludes non-finite balances from net worth', () => {
    component.accounts.set([
      {
        ...mockAccounts[0],
        current_balance: Number.NaN,
      },
      {
        ...mockAccounts[0],
        account_id: 'infinite',
        current_balance: Number.POSITIVE_INFINITY,
      },
    ]);

    const result = component.netWorthSummary();

    expect(result.included).toBe(0);
    expect(result.excluded).toBe(2);
    expect(result.amount).toBe(0);
  });

  /**
   * Test the readable category-label conversion.
   */
  it('formats transaction categories for display', () => {
    expect(component.categoryLabel('FOOD_DINING')).toBe('Food Dining');
    expect(component.categoryLabel('GROCERIES')).toBe('Groceries');
    expect(component.categoryLabel('OTHER')).toBe('Other');
  });

  it('loads category spending when dashboard initializes', () => {
    expect(budgetService.getSpendingByCategory).toHaveBeenCalledOnce();
    expect(component.categorySpendingLoading()).toBe(false);
    expect(component.categorySpendingError()).toBe(false);
    expect(component.expenseChart().total).toBe(100);
  });

  it('calculates category percentages correctly', () => {
    const chart = component.expenseChart();

    expect(chart.segments).toHaveLength(2);

    expect(chart.segments[0].label).toBe('Groceries');
    expect(chart.segments[0].percent).toBe(60);

    expect(chart.segments[1].label).toBe('Food Dining');
    expect(chart.segments[1].percent).toBe(40);
  });

  it('positions the second donut segment after the first', () => {
    const segments = component.expenseChart().segments;

    // The first segment starts at zero.
    expect(segments[0].dashoffset).toBe(0);

    // The second is shifted backward by the length of
    // the first, preventing both segments from overlapping.
    expect(segments[1].dashoffset).toBeLessThan(0);
  });

  it('handles an empty spending response', () => {
    component.categorySpending.set({
      month: '2026-11',
      total: 0,
      categories: [],
    });

    expect(component.expenseChart().total).toBe(0);
    expect(component.expenseChart().segments).toEqual([]);
  });

  it('handles a failed spending request', () => {
    budgetService.getSpendingByCategory.mockReturnValue(
      throwError(() => new Error('Spending API unavailable')),
    );

    component.loadCategorySpending();

    expect(component.categorySpendingError()).toBe(true);
    expect(component.categorySpendingLoading()).toBe(false);
  });

  it('ignores invalid category amounts', () => {
    component.categorySpending.set({
      month: '2026-11',
      total: 100,
      categories: [
        { category: 'GROCERIES', amount: 100 },
        { category: 'SHOPPING', amount: Number.NaN },
        { category: 'OTHER', amount: -20 },
      ],
    });

    const chart = component.expenseChart();

    expect(chart.segments).toHaveLength(1);
    expect(chart.segments[0].category).toBe('GROCERIES');
  });

  /**
   * A 10-year range should include our synthetic
   * 2016 data, while a shorter range excludes it.
   */
  it('filters net-worth snapshots by selected time range', () => {
    // Use relative dates so this test works in future years.
    const now = new Date();
    const recentDate = now.toISOString().slice(0, 10);
    const oldDate = new Date(Date.UTC(now.getUTCFullYear() - 8, now.getUTCMonth(), 1))
      .toISOString()
      .slice(0, 10);

    component.netWorthHistory.set([
      {
        snapshot_date: oldDate,
        amount: 800,
        included_accounts: 1,
        excluded_accounts: 0,
        is_demo: true,
      },
      {
        snapshot_date: recentDate,
        amount: 2100,
        included_accounts: 1,
        excluded_accounts: 0,
        is_demo: false,
      },
    ]);

    // The default range includes both dates.
    component.netWorthRange.set('10Y');

    expect(component.visibleNetWorthHistory()).toHaveLength(2);
    expect(component.netWorthChart().markers).toHaveLength(2);

    // A one-year range excludes the older snapshot.
    component.netWorthRange.set('1Y');

    expect(component.visibleNetWorthHistory()).toHaveLength(1);
    expect(component.netWorthChart().markers).toHaveLength(1);
    expect(component.netWorthChart().markers[0].amount).toBe(2100);
  });

  /**
   * The chart should expose whether its visible data
   * includes simulated sandbox history.
   */
  it('identifies demo history in the selected range', () => {
    const now = new Date();

    const recentDate = now.toISOString().slice(0, 10);
    const oldDate = new Date(Date.UTC(now.getUTCFullYear() - 8, now.getUTCMonth(), 1))
      .toISOString()
      .slice(0, 10);

    component.netWorthHistory.set([
      {
        snapshot_date: oldDate,
        amount: 800,
        included_accounts: 1,
        excluded_accounts: 0,
        is_demo: true,
      },
      {
        snapshot_date: recentDate,
        amount: 2100,
        included_accounts: 1,
        excluded_accounts: 0,
        is_demo: false,
      },
    ]);

    component.netWorthRange.set('10Y');
    expect(component.visibleNetWorthIncludesDemo()).toBe(true);

    component.netWorthRange.set('1Y');
    expect(component.visibleNetWorthIncludesDemo()).toBe(false);
  });
});
