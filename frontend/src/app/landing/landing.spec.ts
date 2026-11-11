import { TestBed } from '@angular/core/testing';
import { Landing } from './landing';

describe('Landing', () => {
  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [Landing],
    }).compileComponents();
  });

  it('should create the landing page', () => {
    const fixture = TestBed.createComponent(Landing);
    const landing = fixture.componentInstance;
    expect(landing).toBeTruthy();
  });

  it('should render title', async () => {
    const fixture = TestBed.createComponent(Landing);
    await fixture.whenStable();
    const compiled = fixture.nativeElement as HTMLElement;
    expect(compiled.querySelector('h1')?.textContent).toContain('Finance Budgeting App');
  });
});
