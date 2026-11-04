import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { provideRouter, Router } from '@angular/router';
import { of, throwError } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { GoalsPage } from './goals';
import { Goal, GoalsService } from '../app/services/goals.service';

describe('GoalsPage', () => {
  let component: GoalsPage;
  let fixture: ComponentFixture<GoalsPage>;
  let httpMock: HttpTestingController;
  let router: Router;

  // Each test gets fresh mock functions, preventing one test
  // from affecting another.
  let goalsService: {
    getGoals: ReturnType<typeof vi.fn>;
    getGoal: ReturnType<typeof vi.fn>;
    createGoal: ReturnType<typeof vi.fn>;
    updateGoal: ReturnType<typeof vi.fn>;
    deleteGoal: ReturnType<typeof vi.fn>;
  };

  const mockGoal: Goal = {
    id: 1,
    name: 'Emergency Fund',
    description: 'Save for unexpected expenses',
    target_amount: 5000,
    target_date: null,
    account_id: 'account-123',
    baseline_balance: 1000,
    current_balance: 2000,
    progress_percent: 25,
    remaining: 3000,
    on_track: null,
    status: 'ACTIVE',
    created_at: '2026-11-04T12:00:00Z',
  };

  beforeEach(async () => {
    goalsService = {
      getGoals: vi.fn().mockReturnValue(of({ goals: [mockGoal] })),
      getGoal: vi.fn(),
      createGoal: vi.fn().mockReturnValue(of(void 0)),
      updateGoal: vi.fn().mockReturnValue(of(mockGoal)),
      deleteGoal: vi.fn().mockReturnValue(of(void 0)),
    };

    await TestBed.configureTestingModule({
      imports: [GoalsPage],
      providers: [
        { provide: GoalsService, useValue: goalsService },
        provideHttpClient(),
        provideHttpClientTesting(),
        provideRouter([]),
      ],
    }).compileComponents();

    httpMock = TestBed.inject(HttpTestingController);
    router = TestBed.inject(Router);

    fixture = TestBed.createComponent(GoalsPage);
    component = fixture.componentInstance;

    // Triggers ngOnInit(), which loads goals and accounts.
    fixture.detectChanges();

    // Respond to the component's account-loading request.
    const request = httpMock.expectOne('http://localhost:3001/api/plaid/accounts');

    request.flush({
      accounts: [
        {
          account_id: 'account-123',
          name: 'Checking',
          account_type: 'depository',
          account_subtype: 'checking',
          mask: '1234',
          current_balance: 2000,
        },
      ],
    });
  });

  afterEach(() => {
    httpMock.verify();
    vi.restoreAllMocks();
  });

  /**
   * Checks that the component loads goals when initialized.
   * Uses a mocked GoalsService response and verifies component state.
   */
  it('should load goals on initialization', () => {
    expect(goalsService.getGoals).toHaveBeenCalled();
    expect(component.goals()).toEqual([mockGoal]);
    expect(component.loading()).toBe(false);
  });

  /**
   * Checks that the component loads eligible bank accounts.
   * Verifies that the depository account is available for goals.
   */
  it('should load connected accounts', () => {
    expect(component.accounts()).toHaveLength(1);
    expect(component.accounts()[0].account_id).toBe('account-123');
    expect(component.accountsLoading()).toBe(false);
  });

  /**
   * Checks that users can open a blank goal form.
   * Verifies that previous form values are cleared.
   */
  it('should open the create goal form', () => {
    component.name = 'Old Goal';

    component.startCreate();

    expect(component.showForm()).toBe(true);
    expect(component.editingId()).toBe(null);
    expect(component.name).toBe('');
  });

  /**
   * Checks that editing a goal fills the form with its data.
   * Verifies the selected ID, name, target and account.
   */
  it('should populate the edit form', () => {
    component.startEdit(mockGoal);

    expect(component.showForm()).toBe(true);
    expect(component.editingId()).toBe(1);
    expect(component.name).toBe('Emergency Fund');
    expect(component.targetAmount).toBe(5000);
    expect(component.accountId).toBe('account-123');
  });

  /**
   * Checks that goals cannot be saved with invalid data.
   * Verifies that validation prevents the service call.
   */
  it('should reject an empty goal name', () => {
    component.startCreate();
    component.name = '';
    component.targetAmount = 5000;
    component.accountId = 'account-123';

    component.saveGoal();

    expect(component.errorMessage()).toContain('goal name');
    expect(goalsService.createGoal).not.toHaveBeenCalled();
  });

  /**
   * Checks that users must choose a linked account.
   * Verifies that missing account information blocks creation.
   */
  it('should reject a missing account', () => {
    component.startCreate();
    component.name = 'Emergency Fund';
    component.targetAmount = 5000;

    component.saveGoal();

    expect(component.errorMessage()).toContain('Select');
    expect(goalsService.createGoal).not.toHaveBeenCalled();
  });

  /**
   * Checks that a valid goal can be created.
   * Verifies the submitted data and refreshed goals list.
   */
  it('should create a goal successfully', () => {
    component.startCreate();
    component.name = 'Vacation';
    component.targetAmount = 6000;
    component.accountId = 'account-123';

    component.saveGoal();

    expect(goalsService.createGoal).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'Vacation',
        target_amount: 6000,
        account_id: 'account-123',
      }),
    );

    expect(component.showForm()).toBe(false);
    expect(component.successMessage()).toBe('Goal created successfully!');

    // Once during initialization, once after saving.
    expect(goalsService.getGoals).toHaveBeenCalledTimes(2);
  });

  /**
   * Checks that editing sends changes to the existing goal.
   * Verifies that updateGoal is used instead of createGoal.
   */
  it('should update an existing goal', () => {
    component.startEdit(mockGoal);
    component.name = 'Updated Emergency Fund';

    component.saveGoal();

    expect(goalsService.updateGoal).toHaveBeenCalledWith(
      1,
      expect.objectContaining({
        name: 'Updated Emergency Fund',
      }),
    );

    expect(goalsService.createGoal).not.toHaveBeenCalled();
    expect(component.successMessage()).toBe('Goal updated successfully!');
  });

  /**
   * Checks that failed saves display an error.
   * Simulates a backend failure without changing stored goals.
   */
  it('should handle a failed goal save', () => {
    goalsService.createGoal.mockReturnValue(throwError(() => new Error('Server unavailable')));

    component.startCreate();
    component.name = 'Vacation';
    component.targetAmount = 6000;
    component.accountId = 'account-123';

    component.saveGoal();

    expect(component.errorMessage()).toContain('Unable to save goal');

    expect(component.saving()).toBe(false);
  });

  /**
   * Checks that archiving updates a goal rather than deleting it.
   * Verifies that the archived flag is sent to the backend.
   */
  it('should archive a goal', () => {
    component.toggleArchived(mockGoal);

    expect(goalsService.updateGoal).toHaveBeenCalledWith(1, { archived: true });

    expect(goalsService.deleteGoal).not.toHaveBeenCalled();
    expect(component.successMessage()).toBe('Goal archived.');
  });

  /**
   * Checks that users can cancel goal deletion.
   * Simulates rejecting the confirmation dialog.
   */
  it('should preserve a goal when deletion is canceled', () => {
    vi.spyOn(window, 'confirm').mockReturnValue(false);

    component.deleteGoal(mockGoal);

    expect(goalsService.deleteGoal).not.toHaveBeenCalled();
  });

  /**
   * Checks that confirmed deletion calls the backend.
   * Verifies that the goals list refreshes afterward.
   */
  it('should delete a goal after confirmation', () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);

    component.deleteGoal(mockGoal);

    expect(goalsService.deleteGoal).toHaveBeenCalledWith(1);
    expect(component.successMessage()).toBe('Goal deleted.');
    expect(goalsService.getGoals).toHaveBeenCalledTimes(2);
  });

  /**
   * Checks that the local status filter selects matching goals.
   * Uses active and completed goals to verify filtering.
   */
  it('should filter goals by status', () => {
    component.goals.set([mockGoal, { ...mockGoal, id: 2, status: 'COMPLETED' }]);

    component.statusFilter = 'COMPLETED';

    expect(component.filteredGoals()).toHaveLength(1);
    expect(component.filteredGoals()[0].id).toBe(2);
  });

  /**
   * Verifies that clicking Create Goal displays the form.
   * Tests the actual HTML event binding rather than calling
   * the component method directly.
   */
  it('should display the form when Create Goal is clicked', () => {
    // Get the rendered HTML.
    const page = fixture.nativeElement as HTMLElement;

    // Initially the form should be hidden.
    expect(page.querySelector('#goalName')).toBeNull();

    // Find the Create Goal button in the page header.
    const createButton = page.querySelector('.page-header .primary-button') as HTMLButtonElement;

    expect(createButton).not.toBeNull();

    // Simulate a real user clicking the button.
    createButton.click();

    // Tell Angular to update the rendered HTML.
    fixture.detectChanges();

    // The form should now exist.
    expect(page.querySelector('#goalName')).not.toBeNull();
    expect(component.showForm()).toBe(true);
  });

  /**
   * Verifies that typing a goal name updates the component.
   * Tests Angular's two-way binding through ngModel.
   */
  it('should update the goal name when the user types', async () => {
    // Open the form and render it.
    component.startCreate();
    fixture.detectChanges();

    // Allow Angular's ngModel directive to initialize completely.
    await fixture.whenStable();
    fixture.detectChanges();

    const page = fixture.nativeElement as HTMLElement;
    const nameInput = page.querySelector('#goalName') as HTMLInputElement;

    expect(nameInput).not.toBeNull();

    // Simulate the user typing into the input.
    nameInput.value = 'New Car';
    nameInput.dispatchEvent(new Event('input', { bubbles: true }));

    // Allow the form binding to synchronize with the component.
    await fixture.whenStable();
    fixture.detectChanges();

    expect(component.name).toBe('New Car');
  });

  /**
   * Verifies that clicking Edit opens the form for the selected goal.
   * Tests the actual HTML button and its Angular event binding.
   */
  it('should open the edit form when Edit is clicked', async () => {
    const page = fixture.nativeElement as HTMLElement;

    // Find the Edit button in the rendered goal card.
    const editButton = Array.from(page.querySelectorAll('button')).find(
      (button) => button.textContent?.trim() === 'Edit',
    );

    expect(editButton).toBeDefined();

    // Simulate the user's click.
    editButton!.click();

    // Update the rendered form and wait for Angular to settle.
    fixture.detectChanges();
    await fixture.whenStable();

    // The component should now be editing the selected goal.
    expect(component.showForm()).toBe(true);
    expect(component.editingId()).toBe(mockGoal.id);
    expect(component.name).toBe(mockGoal.name);

    // The form should appear in the actual HTML.
    expect(page.querySelector('#goalName')).not.toBeNull();
  });

  /**
   * Verifies that clicking Archive sends the correct update.
   * Ensures the button is wired to the archive operation.
   */
  it('should archive a goal when Archive is clicked', () => {
    const page = fixture.nativeElement as HTMLElement;

    const archiveButton = Array.from(page.querySelectorAll('button')).find(
      (button) => button.textContent?.trim() === 'Archive',
    );

    expect(archiveButton).toBeDefined();

    archiveButton!.click();

    expect(goalsService.updateGoal).toHaveBeenCalledWith(mockGoal.id, { archived: true });

    expect(goalsService.deleteGoal).not.toHaveBeenCalled();
  });

  /**
   * Verifies that clicking Delete requests confirmation
   * before sending the delete request to the backend.
   */
  it('should delete a goal when Delete is clicked and confirmed', () => {
    const page = fixture.nativeElement as HTMLElement;

    // Simulate the user accepting the confirmation dialog.
    vi.spyOn(window, 'confirm').mockReturnValue(true);

    const deleteButton = Array.from(page.querySelectorAll('button')).find(
      (button) => button.textContent?.trim() === 'Delete',
    );

    expect(deleteButton).toBeDefined();

    deleteButton!.click();

    // The component should have called our mocked service.
    expect(goalsService.deleteGoal).toHaveBeenCalledWith(mockGoal.id);

    // The goals list should reload after deletion.
    expect(goalsService.getGoals).toHaveBeenCalledTimes(2);
  });
});
