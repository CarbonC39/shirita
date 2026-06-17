import { config } from '@vue/test-utils'
import { i18n } from '../i18n'

// Every mounted component gets i18n automatically — no per-file injection,
// so `$t` / useI18n() never throw "is not a function" in tests.
config.global.plugins = [i18n]
