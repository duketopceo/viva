export function validate(x: string): boolean {
  return x.length > 0;
}

export function signIn(user: string): boolean {
  return validate(user);
}
