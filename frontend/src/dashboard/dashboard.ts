import { Component, signal, inject, OnInit, computed } from '@angular/core';
import { Router } from '@angular/router';
import { AuthService } from '../app/services/auth.service';
import { ProfileService } from '../app/services/profile.service';
import { BudgetService, type Budget } from '../app/services/budget.service';
import { CurrencyPipe } from '@angular/common';

@Component({
  imports: [CurrencyPipe],
  selector: 'app-root',
  styleUrl: './dashboard.css',
  templateUrl: './dashboard.html',
})
export class UserDashboard implements OnInit {
  protected readonly title = signal('dashboard');
  private readonly authService = inject(AuthService);
  private readonly router = inject(Router);

  private readonly profileService = inject(ProfileService);
  private readonly budgetService = inject(BudgetService);

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
   * Load the user's name and budget overview when
   * Angular initializes the dashboard.
   */
  ngOnInit(): void {
    this.loadProfile();
    this.loadBudgetSummary();
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
