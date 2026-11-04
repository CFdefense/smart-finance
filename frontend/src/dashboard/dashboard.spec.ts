import { TestBed } from '@angular/core/testing';
import { provideRouter, Router } from '@angular/router';
import { of, throwError } from 'rxjs';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { UserDashboard } from './dashboard';
import { AuthService } from '../app/services/auth.service';
import { ProfileService } from '../app/services/profile.service';
import { BudgetService, type Budget } from '../app/services/budget.service';

describe('UserDashboard', () => {
  let component: UserDashboard;
  let router: Router;

  // Replace real services with predictable test doubles.
  let profileService: {
    getProfile: ReturnType<typeof vi.fn>;
  };

  let budgetService: {
    getBudgets: ReturnType<typeof vi.fn>;
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

  beforeEach(async () => {
    profileService = {
      getProfile: vi.fn().mockReturnValue(of({ first_name: 'Mitchell' })),
    };

    budgetService = {
      getBudgets: vi.fn().mockReturnValue(of({ month: '2026-11', budgets: mockBudgets })),
    };

    authService = {
      logout: vi.fn().mockReturnValue(of(void 0)),
    };

    await TestBed.configureTestingModule({
      imports: [UserDashboard],
      providers: [
        provideRouter([]),
        { provide: ProfileService, useValue: profileService },
        { provide: BudgetService, useValue: budgetService },
        { provide: AuthService, useValue: authService },
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
});
