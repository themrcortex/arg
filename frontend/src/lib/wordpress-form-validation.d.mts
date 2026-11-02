type WordPressCreationForm = {
  name: string
  domain: string
  version: string
  siteTitle: string
  username: string
  email: string
  password: string
}

export function validateWordPressCreation(form: WordPressCreationForm): string
