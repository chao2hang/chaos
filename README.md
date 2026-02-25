# Chaos 主题（Flarum）

Chaos 是一个用于 Flarum 的主题扩展，提供简洁的论坛界面样式与可定制的主题体验。

## 基本信息

- 包名：`nopj/chaos`
- 类型：`flarum-extension`
- 分类：`theme`
- 许可证：`MIT`
- 兼容版本：`flarum/core ^1.8.0`

## 安装

在 Flarum 根目录执行：

```bash
composer require nopj/chaos
```

安装完成后启用扩展：

```bash
php flarum extension:enable nopj-chaos
php flarum cache:clear
```

## 更新

```bash
composer update nopj/chaos --with-dependencies
php flarum cache:clear
php flarum assets:publish
```

## 可选依赖

本主题可与以下扩展配合使用（可选）：

- `flarum/tags`
- `flarum/approval`
- `sycho/flarum-advanced-extension-categories`

## 支持与反馈

- 官方站点：<https://nopj.cn>
