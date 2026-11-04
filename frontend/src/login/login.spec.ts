import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { of, throwError } from 'rxjs';

import { Login } from './login';
import { AuthService } from '../app/services/auth.service';

describe('Login', () => {
  let component: Login;
  let authService: {
    login: ReturnType<typeof vi.fn>;
  };
  let router: {
    navigate: ReturnType<typeof vi.fn>;
  };

  beforeEach(async () => {
    authService = {
      login: vi.fn().mockReturnValue(of(void 0)),
    };

    router = {
      navigate: vi.fn(),
    };

    await TestBed.configureTestingModule({
      imports: [Login],
      providers: [
        { provide: AuthService, useValue: authService },
        { provide: Router, useValue: router },
      ],
    })
      .overrideComponent(Login, {
        set: { template: '' },
      })
      .compileComponents();

    const fixture = TestBed.createComponent(Login);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  /**
   * Checks that login requires both email and password.
   * Submits empty credentials and verifies an error without calling login.
   */
  it('should reject missing login credentials', () => {
    component.email = '';
    component.password = '';

    component.onLogin();

    expect(component.errorMessage()).toBe('Please enter your email and password.');
    expect(authService.login).not.toHaveBeenCalled();
  });

  /**
   * Checks that valid login trims the email and redirects to the dashboard.
   * Mocks success and verifies submitted credentials, navigation, and state reset.
   */
  it('should submit credentials and navigate after successful login', () => {
    component.email = '  john@example.com  ';
    component.password = 'Password123!';

    component.onLogin();

    expect(authService.login).toHaveBeenCalledWith('john@example.com', 'Password123!');
    expect(router.navigate).toHaveBeenCalledWith(['/dashboard']);
    expect(component.isSubmitting()).toBe(false);
  });

  /**
   * Checks that unauthorized login displays the invalid-credentials message.
   * Mocks a 401 response and verifies the error and submission state reset.
   */
  it('should display an error for invalid credentials', () => {
    authService.login.mockReturnValue(throwError(() => ({ status: 401 })));

    component.email = 'john@example.com';
    component.password = 'WrongPassword';

    component.onLogin();

    expect(component.errorMessage()).toBe('Invalid email or password.');
    expect(component.isSubmitting()).toBe(false);
  });

  /**
   * Checks that server failures show a general login error.
   * Mocks a 500 response and verifies the fallback message.
   */
  it('should display a generic error for server failures', () => {
    authService.login.mockReturnValue(throwError(() => ({ status: 500 })));

    component.email = 'john@example.com';
    component.password = 'Password123!';

    component.onLogin();

    expect(component.errorMessage()).toBe('Unable to log in right now. Please try again.');
  });

  /**
   * Checks that login requests are not repeated while one is pending.
   * Sets isSubmitting and verifies the login service is not called.
   */
  it('should prevent duplicate submissions', () => {
    component.isSubmitting.set(true);
    component.email = 'john@example.com';
    component.password = 'Password123!';

    component.onLogin();

    expect(authService.login).not.toHaveBeenCalled();
  });

  /**
   * Checks that entering only an email does not submit login.
   * Leaves the password empty and verifies the error and absent service call.
   */
  it('should reject a missing password when email is provided', () => {
    component.email = 'john@example.com';
    component.password = '';

    component.onLogin();

    expect(component.errorMessage()).toBe('Please enter your email and password.');
    expect(authService.login).not.toHaveBeenCalled();
  });

  /**
   * Checks that a 400 login response uses the invalid-credentials message.
   * Mocks a 400 response and verifies the error and submission state reset.
   */
  it('should display invalid credentials for a 400 response', () => {
    authService.login.mockReturnValue(throwError(() => ({ status: 400 })));

    component.email = 'john@example.com';
    component.password = 'WrongPassword';

    component.onLogin();

    expect(component.errorMessage()).toBe('Invalid email or password.');
    expect(component.isSubmitting()).toBe(false);
  });
});
