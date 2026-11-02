export const WORDPRESS_VERSION_PATTERN = '(?:0|[1-9][0-9]{0,2})[.](?:0|[1-9][0-9]{0,2})[.](?:0|[1-9][0-9]{0,2})'

const wordpressVersionExpression = new RegExp(`^(?:${WORDPRESS_VERSION_PATTERN})$`)

export function isValidWordPressVersion(value) {
  return wordpressVersionExpression.test(value)
}
