import { Component, signal, inject, OnInit, computed } from '@angular/core';
import { Router } from '@angular/router';
import { AuthService } from '../app/services/auth.service';
import { ProfileService } from '../app/services/profile.service';
import {
  BudgetService,
  type Budget,
  type CategorySpendingResponse,
} from '../app/services/budget.service';
import { CurrencyPipe, DecimalPipe } from '@angular/common';
import {
  PlaidService,
  type PlaidAccount,
  type PlaidTransaction,
  type NetWorthSnapshot,
} from '../app/services/plaid.service';
import { Subscription } from 'rxjs';
import type { OnDestroy } from '@angular/core';

@Component({
  imports: [CurrencyPipe, DecimalPipe],
  selector: 'app-root',
  styleUrl: './dashboard.css',
  templateUrl: './dashboard.html',
})
export class UserDashboard implements OnInit, OnDestroy {
  protected readonly title = signal('dashboard');
  private readonly authService = inject(AuthService);
  private readonly router = inject(Router);

  private readonly profileService = inject(ProfileService);
  private readonly budgetService = inject(BudgetService);

  // Reuse the Plaid service already used by the Profile page.
  private readonly plaidService = inject(PlaidService);

  // Keep track of an active transaction request so it can be
  // cancelled when the user switches between accounts.
  private transactionRequest?: Subscription;

  // Connected financial accounts.
  readonly accounts = signal<PlaidAccount[]>([]);

  /**
   * Calculate net worth using the connected Plaid accounts.
   *
   * Plaid reports balances differently depending on account type:
   * - Depository and investment: balances are assets.
   * - Credit and loan: balances are debts (liabilities).
   *
   * Only USD accounts with available balances are included.
   * Unknown types/currencies are skipped rather than guessed.
   *
   * This represents linked-account net worth, not necessarily
   * the user's complete financial net worth.
   */
  readonly netWorthSummary = computed(() => {
    let assets = 0;
    let liabilities = 0;
    let included = 0;
    let excluded = 0;

    for (const account of this.accounts()) {
      const balance = account.current_balance;
      const type = account.account_type.toLowerCase();
      const currency = account.currency_code?.toUpperCase();

      // Ignore missing balances, unsupported currencies, and
      // non-finite values to avoid misleading calculations.
      if (balance === null || !Number.isFinite(balance) || currency !== 'USD') {
        excluded++;
        continue;
      }

      if (type === 'depository' || type === 'investment') {
        assets += balance;
        included++;
      } else if (type === 'credit' || type === 'loan') {
        liabilities += balance;
        included++;
      } else {
        // Some account types don't have clear balance semantics.
        excluded++;
      }
    }

    return {
      amount: assets - liabilities,
      assets,
      liabilities,
      included,
      excluded,
    };
  });

  /**
   * Historical net-worth snapshots retrieved from Rust.
   *
   * We keep history state independent from the current
   * balance summary, because either API request can fail
   * without preventing the other from displaying.
   */
  readonly netWorthHistory = signal<NetWorthSnapshot[]>([]);
  readonly netWorthHistoryLoading = signal(true);
  readonly netWorthHistoryError = signal(false);

  /**
   * Allowed ranges for the historical net-worth graph.
   *
   * The default is 10 years, making the complete demo
   * history visible when the user first opens the page.
   */
  readonly netWorthRange = signal<'1M' | '3M' | '6M' | '1Y' | '5Y' | '10Y'>('10Y');

  /**
   * Filter existing historical records by the selected
   * number of calendar months.
   *
   * IMPORTANT:
   * - We never create missing historical values.
   * - We filter using dates, not array indexes.
   * - The first day of the starting month is included.
   *
   * Example:
   * If today is November 10, 2026, choosing 1Y starts
   * at November 1, 2025.
   */
  readonly visibleNetWorthHistory = computed(() => {
    const monthsByRange = {
      '1M': 1,
      '3M': 3,
      '6M': 6,
      '1Y': 12,
      '5Y': 60,
      '10Y': 120,
    } as const;

    const months = monthsByRange[this.netWorthRange()];

    // Use the current calendar date as the upper boundary.
    const now = new Date();

    // Start on the first day of the earliest month.
    // Subtracting 120 months for 10Y includes Nov 2016
    // when the current month is November 2026.
    const start = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth() - months, 1));

    const today = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate()));

    // Backend dates have YYYY-MM-DD format, so comparing
    // ISO dates lexicographically is safe here.
    const earliestDate = start.toISOString().slice(0, 10);
    const latestDate = today.toISOString().slice(0, 10);

    return this.netWorthHistory()
      .filter(
        (snapshot) =>
          snapshot.snapshot_date >= earliestDate && snapshot.snapshot_date <= latestDate,
      )
      .sort((a, b) => a.snapshot_date.localeCompare(b.snapshot_date));
  });

  /**
   * Whether any of the currently displayed snapshots
   * are synthetic demonstration values.
   */
  readonly visibleNetWorthIncludesDemo = computed(() =>
    this.visibleNetWorthHistory().some((snapshot) => snapshot.is_demo === true),
  );

  /**
   * Handle the dropdown's native change event.
   *
   * Only supported values can update the range signal.
   */
  onNetWorthRangeChange(event: Event): void {
    const value = (event.target as HTMLSelectElement).value;

    if (
      value === '1M' ||
      value === '3M' ||
      value === '6M' ||
      value === '1Y' ||
      value === '5Y' ||
      value === '10Y'
    ) {
      this.netWorthRange.set(value);
    }
  }

  /**
   * Convert historical financial values into SVG coordinates.
   *
   * Our SVG uses a fixed logical coordinate system:
   * - X: 50 to 590
   * - Y: 35 to 195
   *
   * The browser scales this coordinate system to fit
   * the graph's available width.
   *
   * This is presentation math only. The financial amounts
   * themselves come directly from the Rust API.
   */
  readonly netWorthChart = computed(() => {
    // Calculate chart coordinates from the selected date
    // window, not the full historical database response.
    const history = this.visibleNetWorthHistory();

    if (history.length === 0) {
      return {
        points: '',
        markers: [] as { x: number; y: number; amount: number; date: string }[],
        minValue: 0,
        maxValue: 0,
        firstDate: '',
        lastDate: '',
      };
    }

    // Work out the smallest and largest historical values.
    const values = history.map((snapshot) => snapshot.amount);
    const minimum = Math.min(...values);
    const maximum = Math.max(...values);

    // Add vertical padding so the chart doesn't touch its edges.
    // A constant balance still needs a visible plot range.
    const padding = Math.max((maximum - minimum) * 0.1, 1);
    const minValue = minimum - padding;
    const maxValue = maximum + padding;
    const range = maxValue - minValue;

    const left = 50;
    const right = 590;
    const top = 35;
    const bottom = 195;

    // Convert each real snapshot into a point on the graph.
    const markers = history.map((snapshot) => {
      // Space points according to actual calendar dates,
      // rather than assuming snapshots are one day apart.
      const firstDay = Date.parse(`${history[0].snapshot_date}T00:00:00Z`);
      const lastDay = Date.parse(`${history[history.length - 1].snapshot_date}T00:00:00Z`);
      const currentDay = Date.parse(`${snapshot.snapshot_date}T00:00:00Z`);

      const x =
        firstDay === lastDay
          ? (left + right) / 2
          : left + ((currentDay - firstDay) / (lastDay - firstDay)) * (right - left);

      // SVG Y coordinates grow downward, so invert the value.
      const y = bottom - ((snapshot.amount - minValue) / range) * (bottom - top);

      return {
        x,
        y,
        amount: snapshot.amount,
        date: snapshot.snapshot_date,
      };
    });

    // A polyline draws one connected line through all points.
    const points = markers.map((marker) => `${marker.x},${marker.y}`).join(' ');

    return {
      points,
      markers,
      minValue,
      maxValue,
      firstDate: history[0].snapshot_date,
      lastDate: history[history.length - 1].snapshot_date,
    };
  });

  readonly accountsLoading = signal(true);
  readonly accountsError = signal(false);

  // The account whose transactions are currently expanded.
  // null means no account is selected.
  readonly selectedAccountId = signal<string | null>(null);

  // Transactions for the selected account.
  readonly transactions = signal<PlaidTransaction[]>([]);
  readonly transactionsLoading = signal(false);
  readonly transactionsError = signal(false);

  // The Rust endpoint returns a total count so we can
  // implement pagination without retrieving every transaction.
  readonly transactionTotal = signal(0);

  readonly transactionPage = signal(0);
  readonly transactionPageSize = 20;

  // Calculate the number of pages from the API's total.
  readonly totalTransactionPages = computed(() =>
    Math.ceil(this.transactionTotal() / this.transactionPageSize),
  );

  // Convenient value for disabling the Next button.
  readonly hasNextTransactionPage = computed(
    () => this.transactionPage() + 1 < this.totalTransactionPages(),
  );

  /**
   * The name returned by the authenticated user's profile.
   *
   * We start with an empty value rather than displaying
   * somebody else's name while the request is loading.
   */
  readonly firstName = signal('');
  readonly profileLoading = signal(true);

  /**
   * Budget data for the current month.
   *
   * Spending and remaining amounts are calculated by Rust.
   */
  readonly budgets = signal<Budget[]>([]);
  readonly budgetsLoading = signal(true);
  readonly budgetsError = signal(false);

  /**
   * The combined monthly spending limits for active budgets.
   */
  readonly totalBudget = computed(() =>
    this.budgets()
      .filter((budget) => budget.active)
      .reduce((sum, budget) => sum + budget.monthly_limit, 0),
  );

  /**
   * Total spending in active budget categories.
   */
  readonly totalSpent = computed(() =>
    this.budgets()
      .filter((budget) => budget.active)
      .reduce((sum, budget) => sum + budget.spent, 0),
  );

  /**
   * Remaining money across active budget categories.
   *
   * A negative result indicates spending beyond the
   * combined budget limits.
   */
  readonly totalRemaining = computed(() => this.totalBudget() - this.totalSpent());

  /**
   * Monthly expense data returned by the Rust API.
   *
   * This is separate from budgets because users can spend
   * money in categories for which no budget exists.
   */
  readonly categorySpending = signal<CategorySpendingResponse | null>(null);
  readonly categorySpendingLoading = signal(true);
  readonly categorySpendingError = signal(false);

  /**
   * Returns the current calendar month in YYYY-MM format.
   *
   * We use the local calendar date because the month picker
   * represents the month the user sees on their computer.
   *
   * Example: November 2026 becomes "2026-11".
   */
  private currentExpenseMonth(): string {
    const today = new Date();

    const year = today.getFullYear();
    const month = String(today.getMonth() + 1).padStart(2, '0');

    return `${year}-${month}`;
  }

  /**
   * The month currently displayed in Expenses by Category.
   *
   * Angular signals automatically update the UI whenever
   * their values change.
   */
  readonly selectedExpenseMonth = signal(this.currentExpenseMonth());

  /**
   * Called whenever the user chooses a different month.
   *
   * The native HTML month input returns YYYY-MM.
   */
  onExpenseMonthChange(event: Event): void {
    const input = event.target as HTMLInputElement;
    const month = input.value;

    // Reject malformed dates before requesting the backend.
    if (!/^\d{4}-(0[1-9]|1[0-2])$/.test(month)) {
      return;
    }

    // Do not make unnecessary requests if the selected
    // month hasn't actually changed.
    if (month === this.selectedExpenseMonth()) {
      return;
    }

    this.selectedExpenseMonth.set(month);

    // Reload only the expenses chart.
    // Other dashboard cards remain unchanged.
    this.loadCategorySpending();
  }

  /**
   * Colors used consistently for each spending category.
   *
   * These match the Category enum in our Rust backend.
   */
  private readonly expenseColors: Record<string, string> = {
    GROCERIES: '#6284e2',
    FOOD_DINING: '#20a68a',
    SHOPPING: '#f59e0b',
    TRANSPORTATION: '#9b6ee8',
    ENTERTAINMENT: '#f06f83',
    UTILITIES: '#3b9cba',
    HEALTHCARE: '#d278a6',
    OTHER: '#94a3b8',
  };

  /**
   * Calculate the segments used by the SVG donut chart.
   *
   * An SVG circle can be made into a ring by setting:
   * - fill="none"
   * - stroke="some color"
   * - stroke-width="..."
   *
   * We use stroke-dasharray to control how much of the
   * circle each category occupies, and stroke-dashoffset
   * to position that category after the previous ones.
   *
   * The values come from Rust; this computed signal only
   * converts financial totals into drawing instructions.
   */
  readonly expenseChart = computed(() => {
    const response = this.categorySpending();

    // SVG circle radius: 72 units.
    // Circumference = 2 * PI * radius.
    const circumference = 2 * Math.PI * 72;

    // Avoid drawing misleading segments if data is missing
    // or the backend reports no positive spending.
    if (!response || response.total <= 0) {
      return {
        total: 0,
        month: response?.month ?? '',
        segments: [] as {
          category: string;
          label: string;
          amount: number;
          percent: number;
          color: string;
          dasharray: string;
          dashoffset: number;
        }[],
      };
    }

    let distance = 0;

    const segments = response.categories
      .filter((item) => Number.isFinite(item.amount) && item.amount > 0)
      .map((item) => {
        // Percentage of total spending represented by this category.
        const ratio = item.amount / response.total;

        // Convert that fraction into a length along the circle.
        const length = ratio * circumference;

        const segment = {
          category: item.category,
          label: this.categoryLabel(item.category),
          amount: item.amount,
          percent: ratio * 100,
          color: this.expenseColors[item.category] ?? '#94a3b8',

          // The first number is the visible part of the ring.
          // The second number represents the remaining gap.
          dasharray: `${length} ${circumference}`,

          // Negative offset advances the next segment around
          // the circle, directly after the preceding segment.
          dashoffset: distance === 0 ? 0 : -distance,
        };

        distance += length;
        return segment;
      });

    return {
      total: response.total,
      month: response.month,
      segments,
    };
  });

  /**
   * Load all information needed for the dashboard.
   *
   * Each request has independent loading/error state.
   * This means an account API failure does not prevent
   * the profile or budget summary from rendering.
   */
  ngOnInit(): void {
    this.loadProfile();
    this.loadBudgetSummary();
    this.loadAccounts();
    this.loadCategorySpending();
  }

  /**
   * Cancel pending transaction requests when leaving the page.
   */
  ngOnDestroy(): void {
    this.transactionRequest?.unsubscribe();
  }

  /**
   * Retrieve all connected accounts.
   *
   * We display real balances returned by Plaid rather than
   * hardcoded demonstration data.
   */

  loadAccounts(): void {
    this.accountsLoading.set(true);
    this.accountsError.set(false);

    this.plaidService.getAccounts().subscribe({
      next: (response) => {
        this.accounts.set(response.accounts);
        this.accountsLoading.set(false);

        // The backend has now recorded today's snapshot.
        // Fetch history afterward so the graph includes it.
        this.loadNetWorthHistory();
      },
      error: () => {
        this.accountsError.set(true);
        this.accountsLoading.set(false);

        // A failed account refresh should not hide previously
        // recorded history. Attempt to retrieve it separately.
        this.loadNetWorthHistory();
      },
    });
  }

  /**
   * Expand or collapse an account's transaction list.
   *
   * Selecting a different account resets pagination so
   * the user always starts with its newest transactions.
   */
  selectAccount(accountId: string): void {
    // Cancel any previous request before changing accounts.
    this.transactionRequest?.unsubscribe();

    // Clicking the already selected account closes it.
    if (this.selectedAccountId() === accountId) {
      this.selectedAccountId.set(null);
      this.transactions.set([]);
      this.transactionsLoading.set(false);
      this.transactionsError.set(false);
      this.transactionTotal.set(0);
      this.transactionPage.set(0);
      return;
    }

    this.selectedAccountId.set(accountId);
    this.transactionPage.set(0);
    this.transactions.set([]);
    this.transactionTotal.set(0);

    this.loadTransactions();
  }

  /**
   * Load a single page of transactions for the selected account.
   *
   * Pagination is performed by Rust, not by downloading
   * the user's entire transaction history into Angular.
   */
  loadTransactions(): void {
    const accountId = this.selectedAccountId();

    // Nothing to retrieve if the account is collapsed.
    if (accountId === null) {
      return;
    }

    this.transactionRequest?.unsubscribe();

    this.transactionsLoading.set(true);
    this.transactionsError.set(false);

    const offset = this.transactionPage() * this.transactionPageSize;

    this.transactionRequest = this.plaidService
      .getTransactions(accountId, this.transactionPageSize, offset)
      .subscribe({
        next: (response) => {
          this.transactions.set(response.transactions);
          this.transactionTotal.set(response.total ?? response.transactions.length);
          this.transactionsLoading.set(false);
        },
        error: () => {
          this.transactionsError.set(true);
          this.transactionsLoading.set(false);
        },
      });
  }

  /**
   * Retrieve real historical net-worth values from Rust.
   *
   * An empty array is a legitimate response when the user
   * has no saved snapshots yet.
   */
  loadNetWorthHistory(): void {
    this.netWorthHistoryLoading.set(true);
    this.netWorthHistoryError.set(false);

    this.plaidService.getNetWorthHistory().subscribe({
      next: (response) => {
        this.netWorthHistory.set(response.snapshots);
        this.netWorthHistoryLoading.set(false);
      },
      error: () => {
        this.netWorthHistoryError.set(true);
        this.netWorthHistoryLoading.set(false);
      },
    });
  }

  /**
   * Display the next page only when more transactions exist.
   */
  nextTransactionPage(): void {
    if (this.transactionsLoading() || !this.hasNextTransactionPage()) {
      return;
    }

    this.transactionPage.update((page) => page + 1);
    this.loadTransactions();
  }

  /**
   * Return to the preceding transaction page.
   */
  previousTransactionPage(): void {
    if (this.transactionsLoading() || this.transactionPage() === 0) {
      return;
    }

    this.transactionPage.update((page) => page - 1);
    this.loadTransactions();
  }

  /**
   * Convert a backend category such as FOOD_DINING into
   * a more readable display label: "Food Dining".
   */
  categoryLabel(category: string): string {
    return category
      .split('_')
      .map((word) => word.charAt(0) + word.slice(1).toLowerCase())
      .join(' ');
  }

  /**
   * Retrieve the user's actual first name from Rust.
   */
  private loadProfile(): void {
    this.profileService.getProfile().subscribe({
      next: (profile) => {
        this.firstName.set(profile.first_name.trim());
        this.profileLoading.set(false);
      },
      error: () => {
        this.profileLoading.set(false);
      },
    });
  }

  /**
   * Retrieve the current month's budget information.
   *
   * Rust calculates the spending from Plaid transactions.
   * Angular only combines the returned category values.
   */
  private loadBudgetSummary(): void {
    this.budgetsLoading.set(true);
    this.budgetsError.set(false);

    this.budgetService.getBudgets().subscribe({
      next: (response) => {
        this.budgets.set(response.budgets);
        this.budgetsLoading.set(false);
      },
      error: () => {
        this.budgetsError.set(true);
        this.budgetsLoading.set(false);
      },
    });
  }

  /**
   * Load spending totals across all transaction categories.
   *
   * This request is independent from budget loading so
   * failures in either section don't hide the other.
   */
  loadCategorySpending(): void {
    this.categorySpendingLoading.set(true);
    this.categorySpendingError.set(false);

    // Ask Rust to calculate spending for the chosen month,
    // rather than always using the current month.
    this.budgetService.getSpendingByCategory(this.selectedExpenseMonth()).subscribe({
      next: (response) => {
        this.categorySpending.set(response);
        this.categorySpendingLoading.set(false);
      },
      error: () => {
        this.categorySpendingError.set(true);
        this.categorySpendingLoading.set(false);
      },
    });
  }

  /** Navigate to the Goals page. */
  openGoals(): void {
    this.router.navigate(['/goals']);
  }

  /**
   * Navigates to the dedicated Budgets management page.
   *
   * Both Create Budget and Edit Budget use the same page,
   * which already provides forms for both operations.
   */
  openBudgets(): void {
    this.router.navigate(['/budgets']);
  }

  /** Navigates to the User Profile page. */
  openProfile(): void {
    this.router.navigate(['/profile']);
  }

  /** Logs the user out and navigates to the Login page. */
  onLogout() {
    this.authService.logout().subscribe({
      next: () => {
        this.router.navigate(['/login']);
      },
      error: (err) => {
        console.error('Logout failed:', err);
      },
    });
  }
}
