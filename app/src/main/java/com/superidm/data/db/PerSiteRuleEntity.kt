package com.superidm.data.db

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "per_site_rules")
data class PerSiteRuleEntity(
    @PrimaryKey val domain: String,
    val savePath: String = "",
    val maxSegments: Int = 0,
    val preferredQuality: String = "",
    val proxyHost: String = "",
    val proxyPort: Int = 0,
    val userAgent: String = "",
    val referrer: String = "",
    val enableCookies: Boolean = true,
    val skipDuplicates: Boolean = true,
    val createdAt: Long = System.currentTimeMillis()
)
