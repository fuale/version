# Version

переводы: [en_US](./README.md) | ru-RU

_Как это работает:_

1. Следуйте conventional-commits в вашем репозитории.
2. Когда вы будете готовы к релизу, запустите `version`.

Затем `version` выполнит следующее:

1. Найдет последнюю версию репозитория, просмотрев последний тег git.
2. Изменит версию в файлах (composer.json, package.json, Chart.yaml) на основе ваших коммитов.
3. Сгенерирует список изменений (CHANGELOG.md) на основе ваших коммитов.
4. Создаст коммит, включающий ваши файлы и обновленный CHANGELOG.
5. Создает новый тег с новым номером версии.

_Почему:_ В своей повседневной работе я использую GitLab Ci и развертываю проекты всякий раз, когда создается новый тег.
Из-за этого мне нужен инструмент, который создаст тег, запишет журнал изменений и добавит версию пакета во все файлы
проекта, чтобы я не делал это руками.

## Использование CLI

```console
user@pc:~$ version
```